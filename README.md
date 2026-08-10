# xwayclip

`xwayclip` provides clipboard synchronization from X11 to Wayland. It is intended for native Wayland applications that still use X11 apis for clipboard operations, such as Linux QQ. (腾讯眉目了)

## How it works

`xwayclip` watches X11 `CLIPBOARD` owner changes through XFixes, requests every transferable target, and publishes their distinct contents together as one Wayland data-control source.

Large `INCR` transfers are supported and a content fingerprint is used to suppress repeated snapshots.

## Requirements

- An X11 display reachable through `DISPLAY` with XFixes extension available
- A Wayland compositor reachable through `WAYLAND_DISPLAY`
- Compositor support for `ext-data-control` or `wlr-data-control`

## Installation

From source:

```sh
cargo install xwayclip
```

## Usage

```sh
xwayclip
```

Run `xwayclip --help` for usage.

## Development

```sh
cargo run
```

Set `RUST_LOG=xwayclip=debug` to inspect clipboard changes without logging clipboard contents.

## LICENSE

[MIT](LICENSE). Made with ♥️ by [Ray](https://github.com/so1ve).
