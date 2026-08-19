use std::cmp::min;
use std::collections::HashMap;
use std::io::{ErrorKind, Read};
use std::os::unix::net::UnixStream;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use rustix::event::{PollFd, PollFlags, poll};
use tracing::{debug, trace, warn};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::Event;
use x11rb::protocol::xfixes::{self, SelectionEventMask, SelectionNotifyEvent as XfixesNotify};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ConnectionExt as XprotoConnectionExt,
    CreateWindowAux, EventMask, PropMode, Property, PropertyNotifyEvent, SELECTION_NOTIFY_EVENT,
    SelectionNotifyEvent, SelectionRequestEvent, Timestamp, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;
use x11rb::{COPY_DEPTH_FROM_PARENT, CURRENT_TIME, NONE};

use crate::snapshot::{Offer, Snapshot, is_x11_target};
use crate::{ClipboardUpdate, Config, WorkerEvent, X11Command};

const PROPERTY_CHUNK_LONGS: u32 = 16 * 1024;
const MAX_TARGET_COUNT: usize = 4096;
const TARGET_LIST_LIMIT: usize = MAX_TARGET_COUNT * size_of::<Atom>();
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(2);

struct Atoms {
    clipboard: Atom,
    incr: Atom,
    property: Atom,
    targets: Atom,
}

struct PropertyValue {
    type_: Atom,
    format: u8,
    data: Vec<u8>,
    exceeded_limit: bool,
}

struct OwnedTarget {
    atom: Atom,
    source: usize,
}

struct OwnedSelection {
    snapshot: Rc<Snapshot>,
    targets: Vec<OwnedTarget>,
}

struct IncrTransfer {
    snapshot: Rc<Snapshot>,
    source: usize,
    offset: usize,
    target: Atom,
}

struct ClipboardBridge {
    conn: RustConnection,
    window: Window,
    atoms: Atoms,
    config: Config,
    events: Sender<WorkerEvent>,
    max_property_bytes: usize,
    pending_change: Option<XfixesNotify>,
    owned: Option<OwnedSelection>,
    incr_transfers: HashMap<(Window, Atom), IncrTransfer>,
}

pub fn run(
    config: Config,
    events: Sender<WorkerEvent>,
    commands: &Receiver<X11Command>,
    mut wake: UnixStream,
) -> Result<()> {
    let mut bridge = ClipboardBridge::new(config, events)?;
    bridge.events.send(WorkerEvent::Ready).unwrap();

    bridge.event_loop(commands, &mut wake)
}

impl ClipboardBridge {
    fn new(config: Config, events: Sender<WorkerEvent>) -> Result<Self> {
        let (conn, screen_number) = x11rb::connect(None).context("cannot connect to DISPLAY")?;
        let screen = &conn.setup().roots[screen_number];
        let root = screen.root;
        let root_visual = screen.root_visual;
        let window = conn.generate_id()?;

        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            root_visual,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?
        .check()?;

        let atoms = Atoms {
            clipboard: intern_atom(&conn, b"CLIPBOARD")?,
            incr: intern_atom(&conn, b"INCR")?,
            property: intern_atom(&conn, b"_XWAYCLIP_SELECTION")?,
            targets: intern_atom(&conn, b"TARGETS")?,
        };

        xfixes::query_version(&conn, 5, 0)?.reply()?;
        xfixes::select_selection_input(
            &conn,
            window,
            atoms.clipboard,
            SelectionEventMask::SET_SELECTION_OWNER
                | SelectionEventMask::SELECTION_WINDOW_DESTROY
                | SelectionEventMask::SELECTION_CLIENT_CLOSE,
        )?
        .check()?;

        let max_property_bytes = conn.maximum_request_bytes() - 32;
        conn.flush()?;

        Ok(Self {
            conn,
            window,
            atoms,
            config,
            events,
            max_property_bytes,
            pending_change: None,
            owned: None,
            incr_transfers: HashMap::new(),
        })
    }

    fn event_loop(&mut self, commands: &Receiver<X11Command>, wake: &mut UnixStream) -> Result<()> {
        loop {
            loop {
                match commands.try_recv() {
                    Ok(X11Command::Set(snapshot)) => self.set_clipboard(snapshot)?,
                    Ok(X11Command::Clear) => self.clear_clipboard()?,
                    Err(TryRecvError::Disconnected) => return Ok(()),
                    Err(TryRecvError::Empty) => break,
                }
            }

            self.drain_events()?;
            if let Some(change) = self.pending_change.take() {
                self.process_selection_change(change)?;
                continue;
            }

            let mut poll_fds = [
                PollFd::new(self.conn.stream(), PollFlags::IN),
                PollFd::new(&*wake, PollFlags::IN),
            ];
            loop {
                match poll(&mut poll_fds, None) {
                    Ok(_) => break,
                    Err(rustix::io::Errno::INTR) => {}
                    Err(error) => {
                        return Err(error).context("failed to poll X11 clipboard descriptors");
                    }
                }
            }
            let x11_revents = poll_fds[0].revents();
            let wake_revents = poll_fds[1].revents();

            if x11_revents.intersects(PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL)
                && !x11_revents.contains(PollFlags::IN)
            {
                bail!("X11 connection closed");
            }
            if wake_revents.contains(PollFlags::HUP) {
                return Ok(());
            }
            if wake_revents.contains(PollFlags::IN) {
                drain_wake(wake)?;
            }
        }
    }

    fn handle_event(&mut self, event: &Event) {
        match event {
            Event::XfixesSelectionNotify(event) => self.remember_change(*event),
            Event::SelectionRequest(event) => {
                if let Err(error) = self.handle_selection_request(*event) {
                    warn!(%error, target = event.target, "failed to serve X11 selection request");
                }
            }
            Event::SelectionClear(_) => self.owned = None,
            Event::PropertyNotify(event) if event.state == Property::DELETE => {
                if let Err(error) = self.advance_incr_transfer(*event) {
                    warn!(%error, "failed to advance X11 INCR transfer");
                }
            }
            _ => {}
        }
    }

    fn drain_events(&mut self) -> Result<()> {
        while let Some(event) = self.conn.poll_for_event()? {
            self.handle_event(&event);
        }

        Ok(())
    }

    fn process_selection_change(&mut self, change: XfixesNotify) -> Result<()> {
        if change.owner == NONE {
            trace!("X11 clipboard was cleared");
            self.events
                .send(WorkerEvent::X11(ClipboardUpdate::Cleared))
                .unwrap();

            return Ok(());
        }

        match self.capture(&change) {
            Ok(Some(snapshot)) => {
                self.events
                    .send(WorkerEvent::X11(ClipboardUpdate::Set(snapshot)))
                    .unwrap();
            }
            Ok(None) => debug!("discarded a stale or empty X11 clipboard snapshot"),
            Err(error) => warn!(%error, "could not capture the complete X11 clipboard snapshot"),
        }

        Ok(())
    }

    fn set_clipboard(&mut self, snapshot: Snapshot) -> Result<()> {
        let snapshot = Rc::new(snapshot);
        let targets = snapshot
            .x11_targets()
            .into_iter()
            .map(|(mime_type, source)| {
                Ok(OwnedTarget {
                    atom: intern_atom(&self.conn, mime_type.as_bytes())?,
                    source,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        self.owned = Some(OwnedSelection { snapshot, targets });

        self.conn
            .set_selection_owner(self.window, self.atoms.clipboard, CURRENT_TIME)?
            .check()?;
        self.conn.flush()?;

        Ok(())
    }

    fn clear_clipboard(&mut self) -> Result<()> {
        self.conn
            .set_selection_owner(NONE, self.atoms.clipboard, CURRENT_TIME)?
            .check()?;
        self.owned = None;
        self.conn.flush()?;

        Ok(())
    }

    fn handle_selection_request(&mut self, request: SelectionRequestEvent) -> Result<()> {
        if request.property == NONE {
            return self.send_selection_notify(request, NONE);
        }

        let property = match self.write_selection_property(&request) {
            Ok(()) => request.property,
            Err(error) => {
                warn!(%error, target = request.target, "refusing X11 selection target");
                NONE
            }
        };

        self.send_selection_notify(request, property)
    }

    fn write_selection_property(&mut self, request: &SelectionRequestEvent) -> Result<()> {
        let owned = self
            .owned
            .as_ref()
            .context("X11 selection request arrived without owned clipboard data")?;

        if request.target == self.atoms.targets {
            let mut targets = Vec::with_capacity(owned.targets.len() + 1);
            targets.push(self.atoms.targets);
            targets.extend(owned.targets.iter().map(|target| target.atom));
            self.conn
                .change_property32(
                    PropMode::REPLACE,
                    request.requestor,
                    request.property,
                    AtomEnum::ATOM,
                    &targets,
                )?
                .check()?;

            return Ok(());
        }

        let target = owned
            .targets
            .iter()
            .find(|target| target.atom == request.target)
            .context("requested X11 target is not offered")?;
        let snapshot = Rc::clone(&owned.snapshot);
        let source = target.source;
        let data = &snapshot.offers()[source].data;

        if data.len() <= self.max_property_bytes {
            self.conn
                .change_property8(
                    PropMode::REPLACE,
                    request.requestor,
                    request.property,
                    request.target,
                    data,
                )?
                .check()?;

            return Ok(());
        }

        let announced_size = u32::try_from(data.len())
            .context("X11 INCR transfer is larger than the protocol can announce")?;
        self.conn
            .change_window_attributes(
                request.requestor,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
            )?
            .check()?;
        self.conn
            .change_property32(
                PropMode::REPLACE,
                request.requestor,
                request.property,
                self.atoms.incr,
                &[announced_size],
            )?
            .check()?;

        trace!(
            target = request.target,
            bytes = data.len(),
            "starting X11 INCR transfer"
        );
        self.incr_transfers.insert(
            (request.requestor, request.property),
            IncrTransfer {
                snapshot,
                source,
                offset: 0,
                target: request.target,
            },
        );

        Ok(())
    }

    fn send_selection_notify(&self, request: SelectionRequestEvent, property: Atom) -> Result<()> {
        let event = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property,
        };
        self.conn
            .send_event(false, request.requestor, EventMask::NO_EVENT, event)?
            .check()?;
        self.conn.flush()?;

        Ok(())
    }

    fn advance_incr_transfer(&mut self, event: PropertyNotifyEvent) -> Result<()> {
        let key = (event.window, event.atom);
        let Some(mut transfer) = self.incr_transfers.remove(&key) else {
            return Ok(());
        };

        let data = &transfer.snapshot.offers()[transfer.source].data;
        let end = min(transfer.offset + self.max_property_bytes, data.len());
        self.conn
            .change_property8(
                PropMode::REPLACE,
                event.window,
                event.atom,
                transfer.target,
                &data[transfer.offset..end],
            )?
            .check()?;
        self.conn.flush()?;

        if transfer.offset == end {
            trace!(target = transfer.target, "completed X11 INCR transfer");
            if !self
                .incr_transfers
                .keys()
                .any(|(window, _)| *window == event.window)
            {
                self.conn
                    .change_window_attributes(
                        event.window,
                        &ChangeWindowAttributesAux::new().event_mask(EventMask::NO_EVENT),
                    )?
                    .check()?;
            }
        } else {
            transfer.offset = end;
            self.incr_transfers.insert(key, transfer);
        }

        Ok(())
    }

    fn capture(&mut self, change: &XfixesNotify) -> Result<Option<Snapshot>> {
        trace!(owner = change.owner, "capturing X11 clipboard targets");

        let targets =
            self.request_target(change.timestamp, self.atoms.targets, TARGET_LIST_LIMIT)?;
        ensure!(
            targets.format == 32,
            "X11 TARGETS response is not 32-bit ATOM data"
        );
        ensure!(
            !targets.exceeded_limit,
            "X11 owner advertised more than {MAX_TARGET_COUNT} targets"
        );

        let target_atoms = decode_atoms(&targets.data);
        let mut offers = Vec::with_capacity(target_atoms.len());
        let mut total_bytes = 0_usize;

        for atom in target_atoms {
            if self.pending_change.is_some() {
                return Ok(None);
            }

            let name = self.conn.get_atom_name(atom)?.reply()?.name;
            let Ok(mime_type) = String::from_utf8(name) else {
                trace!(atom, "skipping non-UTF-8 X11 target");
                continue;
            };
            if !is_x11_target(&mime_type) {
                trace!(%mime_type, "skipping X11 protocol target");
                continue;
            }

            let remaining_total = self.config.max_total_bytes - total_bytes;
            let target_limit = min(self.config.max_target_bytes, remaining_total);
            let value = self.request_target(change.timestamp, atom, target_limit)?;
            ensure!(
                !value.exceeded_limit,
                "clipboard target {mime_type:?} exceeds its size limit"
            );

            total_bytes += value.data.len();
            trace!(%mime_type, bytes = value.data.len(), "captured X11 clipboard target");
            offers.push(Offer {
                mime_type,
                data: value.data,
            });
        }

        self.drain_events()?;
        if self.pending_change.is_some() {
            return Ok(None);
        }

        Ok((!offers.is_empty()).then(|| Snapshot::new(offers)))
    }

    fn request_target(
        &mut self,
        timestamp: Timestamp,
        target: Atom,
        size_limit: usize,
    ) -> Result<PropertyValue> {
        let deadline = Instant::now() + self.config.transfer_timeout;

        self.conn
            .delete_property(self.window, self.atoms.property)?
            .check()?;
        self.conn
            .convert_selection(
                self.window,
                self.atoms.clipboard,
                target,
                self.atoms.property,
                timestamp,
            )?
            .check()?;

        self.wait_for_selection_notify(deadline)?;

        let initial = self.take_property(size_limit)?;
        if initial.type_ != self.atoms.incr {
            return Ok(initial);
        }

        let mut exceeded_limit = initial.exceeded_limit;
        if !exceeded_limit {
            ensure!(
                initial.format == 32 && initial.data.len() >= 4,
                "X11 INCR response does not contain a 32-bit size"
            );
            exceeded_limit =
                u32::from_ne_bytes(*initial.data.first_chunk::<4>().unwrap()) as usize > size_limit;
        }
        let mut data = Vec::new();
        let mut result_type = NONE;
        let mut result_format = 0;

        loop {
            self.wait_for_property_update(deadline)?;
            let remaining = if exceeded_limit {
                0
            } else {
                size_limit - data.len()
            };
            let chunk = self.take_property(remaining)?;

            if chunk.data.is_empty() && !chunk.exceeded_limit {
                break;
            }

            if result_type == NONE {
                result_type = chunk.type_;
                result_format = chunk.format;
            } else {
                ensure!(
                    chunk.type_ == result_type && chunk.format == result_format,
                    "X11 INCR transfer changed its property type or format"
                );
            }

            if chunk.exceeded_limit {
                exceeded_limit = true;
                data.clear();
            } else if !exceeded_limit {
                data.extend_from_slice(&chunk.data);
            }
        }

        Ok(PropertyValue {
            type_: result_type,
            format: result_format,
            data,
            exceeded_limit,
        })
    }

    fn wait_for_selection_notify(&mut self, deadline: Instant) -> Result<()> {
        loop {
            match self.poll_event_until(deadline)? {
                Event::SelectionNotify(event) => {
                    ensure!(event.property != NONE, "X11 owner refused clipboard target");

                    return Ok(());
                }
                event => self.handle_event(&event),
            }
        }
    }

    fn wait_for_property_update(&mut self, deadline: Instant) -> Result<()> {
        loop {
            match self.poll_event_until(deadline)? {
                Event::PropertyNotify(event)
                    if event.window == self.window
                        && event.atom == self.atoms.property
                        && event.state == Property::NEW_VALUE =>
                {
                    return Ok(());
                }
                event => self.handle_event(&event),
            }
        }
    }

    fn poll_event_until(&self, deadline: Instant) -> Result<Event> {
        loop {
            if let Some(event) = self.conn.poll_for_event()? {
                return Ok(event);
            }
            if Instant::now() >= deadline {
                bail!("timed out waiting for X11 clipboard transfer");
            }
            thread::sleep(EVENT_POLL_INTERVAL);
        }
    }

    fn take_property(&self, size_limit: usize) -> Result<PropertyValue> {
        let mut offset = 0_u32;
        let mut type_ = NONE;
        let mut format = 0_u8;
        let mut data = Vec::new();
        let mut exceeded_limit = false;

        loop {
            let reply = self
                .conn
                .get_property(
                    true,
                    self.window,
                    self.atoms.property,
                    AtomEnum::ANY,
                    offset,
                    PROPERTY_CHUNK_LONGS,
                )?
                .reply()?;

            if offset == 0 {
                type_ = reply.type_;
                format = reply.format;
            } else {
                ensure!(
                    reply.type_ == type_ && reply.format == format,
                    "X11 property changed while it was being read"
                );
            }

            if !exceeded_limit {
                let remaining = size_limit - data.len();
                if reply.value.len() <= remaining {
                    data.extend_from_slice(&reply.value);
                } else {
                    exceeded_limit = true;
                    data.clear();
                }
            }

            if reply.bytes_after == 0 {
                break;
            }
            ensure!(
                !reply.value.is_empty(),
                "X11 property read made no progress"
            );

            offset += reply.value.len().div_ceil(4) as u32;
        }

        Ok(PropertyValue {
            type_,
            format,
            data,
            exceeded_limit,
        })
    }

    fn remember_change(&mut self, event: XfixesNotify) {
        self.pending_change = (event.owner != self.window).then_some(event);
    }
}

fn intern_atom(conn: &RustConnection, name: &[u8]) -> Result<Atom> {
    Ok(conn.intern_atom(false, name)?.reply()?.atom)
}

fn decode_atoms(bytes: &[u8]) -> Vec<Atom> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| u32::from_ne_bytes(*chunk))
        .collect()
}

fn drain_wake(wake: &mut UnixStream) -> Result<()> {
    let mut buffer = [0_u8; 64];
    loop {
        match wake.read(&mut buffer) {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => {
                return Err(error).context("failed to drain X11 worker wake pipe");
            }
        }
    }
}
