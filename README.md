# xwayclip

[![CI](https://github.com/so1ve/xwayclip/actions/workflows/ci.yml/badge.svg)](https://github.com/so1ve/xwayclip/actions/workflows/ci.yml)
[![Cachix Cache](https://img.shields.io/badge/cachix-so1ve-blue.svg)](https://so1ve.cachix.org)

xwayclip synchronizes the regular clipboard in both directions between X11 and Wayland. It is designed for mixed-protocol applications such as Linux QQ (司马腾讯不适配 Wayland剪贴板), where one process can use native Wayland for its windows while still reading or writing clipboard data through X11.

Unlike focus-dependent Xwayland clipboard integration, xwayclip owns an independent data-control connection. It eagerly captures every advertised format before publishing the same snapshot on the other display protocol.

## How it works

`xwayclip` runs two clipboard workers:

- The X11 worker watches `CLIPBOARD` owner changes through XFixes and captures every transferable target.
- The Wayland worker watches data-control selection changes and captures every advertised MIME type.

The bridge publishes each complete snapshot on the opposite side. On X11 it becomes the selection owner and serves `TARGETS` plus individual format requests; on Wayland it provides a multi-MIME data-control source.

Large X11 transfers use `INCR` in both directions. Normalized content fingerprints suppress the echo created by the two clipboard protocols and their text aliases. Clearing either clipboard clears the other one as well.

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
