# xwayclip

[![Cachix Cache](https://img.shields.io/badge/cachix-so1ve-blue.svg)](https://so1ve.cachix.org)

`xwayclip` provides clipboard synchronization from X11 to Wayland. It is intended for native Wayland applications that still use X11 apis for clipboard operations, such as Linux QQ. (腾讯眉目了)

## How it works

`xwayclip` watches X11 `CLIPBOARD` owner changes through XFixes, requests every transferable target, and publishes their distinct contents together as one Wayland data-control source.

Large `INCR` transfers are supported and a content fingerprint is used to suppress repeated snapshots.

## Requirements

- An X11 display reachable through `DISPLAY` with XFixes extension available
- A Wayland compositor reachable through `WAYLAND_DISPLAY`
- Compositor support for `ext-data-control` or `wlr-data-control`

Built and tested on nightly rust with `niri` compositor. Should work on any compositor with `ext-data-control` or `wlr-data-control` support and stable rust.

## Installation

### From Source

With Cargo:

```sh
cargo install xwayclip
```

### Nix

Run directly:

```sh
nix run github:so1ve/xwayclip
```

With a Nix flake, add `xwayclip` to the inputs:

```nix
inputs.xwayclip.url = "github:so1ve/xwayclip";
```

Then add the package to a NixOS module:

```nix
{ inputs, pkgs, ... }:

{
  environment.systemPackages = [
    inputs.xwayclip.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];
}
```

Alternatively, use an overlay to make `pkgs.xwayclip` available:

```nix
{ inputs, pkgs, ... }:

{
  nixpkgs.overlays = [
    inputs.xwayclip.overlays.default
  ];

  environment.systemPackages = [
    pkgs.xwayclip
  ];
}
```

### Cachix

```nix
nix.settings = {
  extra-substituters = [ "https://so1ve.cachix.org" ];
  extra-trusted-public-keys = [
    "so1ve.cachix.org-1:51jcW4FkJhiLcqPsiUx3nglRP469les8F9zjFxio1nw="
  ];
};
```

## Usage

```sh
xwayclip
```

Run `xwayclip --help` for usage.

## Development

You can use `devenv` on Nix:

```sh
devenv shell
```

Run locally:

```sh
cargo run
```

Set `RUST_LOG=xwayclip=debug` to inspect clipboard changes without logging clipboard contents.

## LICENSE

[MIT](LICENSE). Made with ♥️ by [Ray](https://github.com/so1ve).
