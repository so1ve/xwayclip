use std::cmp::min;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use tracing::{debug, trace, warn};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xfixes::{self, SelectionEventMask, SelectionNotifyEvent as XfixesNotify};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, Property, Timestamp, Window,
    WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::{COPY_DEPTH_FROM_PARENT, CURRENT_TIME, NONE};

use crate::Config;
use crate::snapshot::{Offer, Snapshot};

const PROPERTY_CHUNK_LONGS: u32 = 16 * 1024;
const MAX_TARGET_COUNT: usize = 4096;
const TARGET_LIST_LIMIT: usize = MAX_TARGET_COUNT * size_of::<Atom>();
const SNAPSHOT_ATTEMPTS: usize = 2;
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(2);

const CONTROL_TARGETS: &[&str] = &[
    "ATOM",
    "ATOM_PAIR",
    "DELETE",
    "INCR",
    "INSERT_PROPERTY",
    "INSERT_SELECTION",
    "MULTIPLE",
    "SAVE_TARGETS",
    "TARGETS",
    "TIMESTAMP",
];

struct SelectionChange {
    owner: Window,
    timestamp: Timestamp,
}

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

pub struct ClipboardWatcher {
    conn: RustConnection,
    window: Window,
    atoms: Atoms,
    config: Config,
    pending_change: Option<SelectionChange>,
}

impl ClipboardWatcher {
    pub fn new(config: Config, sync_current_owner: bool) -> Result<Self> {
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

        let pending_change = if sync_current_owner {
            let owner = conn.get_selection_owner(atoms.clipboard)?.reply()?.owner;
            (owner != NONE).then_some(SelectionChange {
                owner,
                timestamp: CURRENT_TIME,
            })
        } else {
            None
        };

        Ok(Self {
            conn,
            window,
            atoms,
            config,
            pending_change,
        })
    }

    pub fn next_snapshot(&mut self) -> Result<Snapshot> {
        loop {
            let change = if let Some(change) = self.pending_change.take() {
                change
            } else {
                self.wait_for_change()?
            };

            if change.owner == NONE {
                trace!("X11 clipboard was cleared");
                continue;
            }

            for attempt in 1..=SNAPSHOT_ATTEMPTS {
                match self.capture(&change) {
                    Ok(Some(snapshot)) => return Ok(snapshot),
                    Ok(None) => {
                        debug!("discarded a stale or empty X11 clipboard snapshot");
                        break;
                    }
                    Err(error)
                        if attempt < SNAPSHOT_ATTEMPTS && self.selection_is_current(&change)? =>
                    {
                        warn!(
                            %error,
                            next_attempt = attempt + 1,
                            "could not capture the complete X11 clipboard snapshot; retrying"
                        );
                    }
                    Err(error) => {
                        warn!(%error, "could not capture the complete X11 clipboard snapshot");
                        break;
                    }
                }
            }
        }
    }

    fn wait_for_change(&mut self) -> Result<SelectionChange> {
        loop {
            let event = self.conn.wait_for_event()?;
            if let Event::XfixesSelectionNotify(event) = event
                && let Some(change) = self.selection_change(event)
            {
                return Ok(change);
            }
        }
    }

    fn capture(&mut self, change: &SelectionChange) -> Result<Option<Snapshot>> {
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
        if self.has_pending_change()? {
            return Ok(None);
        }

        let target_atoms = decode_atoms(&targets.data);

        let mut offers = Vec::with_capacity(target_atoms.len());
        let mut total_bytes = 0_usize;

        for atom in target_atoms {
            if self.has_pending_change()? {
                return Ok(None);
            }

            let mime_type = self.atom_name(atom)?;
            if !is_transferable_target(&mime_type) {
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

        if !self.selection_is_current(change)? {
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

        self.wait_for_selection_notify(target, deadline)?;

        let initial = self.take_property(size_limit)?;
        if initial.type_ != self.atoms.incr {
            return Ok(initial);
        }

        let announced_size = initial
            .data
            .first_chunk::<4>()
            .map(|bytes| u32::from_ne_bytes(*bytes) as usize)
            .unwrap_or(0);
        let mut exceeded_limit = announced_size > size_limit || initial.exceeded_limit;
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

    fn wait_for_selection_notify(&mut self, target: Atom, deadline: Instant) -> Result<()> {
        loop {
            match self.poll_event_until(deadline)? {
                Event::SelectionNotify(event)
                    if event.requestor == self.window
                        && event.selection == self.atoms.clipboard
                        && event.target == target =>
                {
                    ensure!(
                        event.property != NONE,
                        "X11 owner refused target atom {target}"
                    );
                    return Ok(());
                }
                Event::XfixesSelectionNotify(event) => self.remember_change(event),
                _ => {}
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
                Event::XfixesSelectionNotify(event) => self.remember_change(event),
                _ => {}
            }
        }
    }

    fn poll_event_until(&self, deadline: Instant) -> Result<Event> {
        loop {
            if let Some(event) = self.conn.poll_for_event()? {
                return Ok(event);
            }
            if Instant::now() >= deadline {
                bail!("timed out waiting for X11 selection owner");
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
        if let Some(change) = self.selection_change(event) {
            self.pending_change = Some(change);
        }
    }

    fn has_pending_change(&mut self) -> Result<bool> {
        while let Some(event) = self.conn.poll_for_event()? {
            if let Event::XfixesSelectionNotify(event) = event {
                self.remember_change(event);
            }
        }

        Ok(self.pending_change.is_some())
    }

    fn selection_is_current(&mut self, change: &SelectionChange) -> Result<bool> {
        if self.has_pending_change()? {
            return Ok(false);
        }

        let current_owner = self
            .conn
            .get_selection_owner(self.atoms.clipboard)?
            .reply()?
            .owner;
        if current_owner != change.owner {
            debug!(
                expected = change.owner,
                actual = current_owner,
                "X11 clipboard owner changed"
            );
            return Ok(false);
        }

        Ok(!self.has_pending_change()?)
    }

    fn selection_change(&self, event: XfixesNotify) -> Option<SelectionChange> {
        (event.selection == self.atoms.clipboard).then_some(SelectionChange {
            owner: event.owner,
            timestamp: event.timestamp,
        })
    }

    fn atom_name(&self, atom: Atom) -> Result<String> {
        let bytes = self.conn.get_atom_name(atom)?.reply()?.name;
        let name = String::from_utf8(bytes).context("X11 target name is not valid UTF-8")?;
        ensure!(!name.is_empty(), "X11 target name is empty");
        ensure!(!name.contains('\0'), "X11 target name contains \\0");
        Ok(name)
    }
}

fn intern_atom(conn: &RustConnection, name: &[u8]) -> Result<Atom> {
    Ok(conn.intern_atom(false, name)?.reply()?.atom)
}

fn decode_atoms(bytes: &[u8]) -> Vec<Atom> {
    let (chunks, remainder) = bytes.as_chunks::<4>();
    debug_assert!(remainder.is_empty());

    chunks
        .iter()
        .map(|chunk| u32::from_ne_bytes(*chunk))
        .collect()
}

fn is_transferable_target(name: &str) -> bool {
    !CONTROL_TARGETS.contains(&name)
}
