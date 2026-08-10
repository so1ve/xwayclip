# xwayclip

[![CI](https://github.com/so1ve/xwayclip/actions/workflows/ci.yml/badge.svg)](https://github.com/so1ve/xwayclip/actions/workflows/ci.yml)
[![Cachix Cache](https://img.shields.io/badge/cachix-so1ve-blue.svg)](https://so1ve.cachix.org)

xwayland/xwayland-satellite normally provides bidirectional clipboard integration between X11 and Wayland. xwayclip offers an alternative X11-to-Wayland path for applications whose clipboard formats are not forwarded reliably, like Linux QQ. It eagerly captures every advertised X11 format and publishes the resulting snapshot to Wayland. Wayland-to-X11 synchronization remains handled by the existing Xwayland integration.

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

Import the home manager module to enable the service:

```nix
{ inputs, ... }:

{
  imports = [ inputs.xwayclip.homeManagerModules.default ];

  services.xwayclip.enable = true;
}
```

Additional command-line arguments can be configured with `services.xwayclip.extraArgs`.

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
