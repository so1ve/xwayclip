# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.3](https://github.com/so1ve/xwayclip/compare/v0.2.2...v0.2.3) - 2026-11-27

### Fixed

- Preserve Wayland clipboard data across SPICE ([#14](https://github.com/so1ve/xwayclip/pull/14))

### Other

- *(deps)* update rust crate blake3 to v1.8.8 ([#33](https://github.com/so1ve/xwayclip/pull/33))
- *(deps)* update rust crate clap to v4.6.10 ([#32](https://github.com/so1ve/xwayclip/pull/32))
- *(deps)* update rust crate anyhow to v1.0.105 ([#31](https://github.com/so1ve/xwayclip/pull/31))
- *(deps)* lock file maintenance ([#30](https://github.com/so1ve/xwayclip/pull/30))
- pin firn@v1 and push explicit build outputs
- migrate binary cache from oranc to firn
- *(deps)* update rust crate clap to v4.6.9 ([#29](https://github.com/so1ve/xwayclip/pull/29))
- *(deps)* update rust crate clap to v4.6.8 ([#28](https://github.com/so1ve/xwayclip/pull/28))
- *(deps)* lock file maintenance ([#27](https://github.com/so1ve/xwayclip/pull/27))
- migrate Nix cache to GHCR
- *(deps)* lock file maintenance ([#26](https://github.com/so1ve/xwayclip/pull/26))
- *(deps)* lock file maintenance ([#25](https://github.com/so1ve/xwayclip/pull/25))
- *(deps)* update rust crate rustix to v1.1.6 ([#24](https://github.com/so1ve/xwayclip/pull/24))
- *(deps)* lock file maintenance ([#22](https://github.com/so1ve/xwayclip/pull/22))
- *(deps)* lock file maintenance ([#21](https://github.com/so1ve/xwayclip/pull/21))
- *(deps)* lock file maintenance ([#20](https://github.com/so1ve/xwayclip/pull/20))
- mention linuxqq-wayland-fix's clipboard sync limits
- mention linuxqq-wayland-fix as a more comprehensive alternative
- *(deps)* lock file maintenance ([#19](https://github.com/so1ve/xwayclip/pull/19))
- remove reductant build step
- *(deps)* lock file maintenance ([#17](https://github.com/so1ve/xwayclip/pull/17))
- *(deps)* lock file maintenance ([#16](https://github.com/so1ve/xwayclip/pull/16))
- *(deps)* update rust crate rustix to v1.1.5 ([#15](https://github.com/so1ve/xwayclip/pull/15))
- *(deps)* update rust crate clap to v4.6.7 ([#13](https://github.com/so1ve/xwayclip/pull/13))
- *(deps)* lock file maintenance ([#12](https://github.com/so1ve/xwayclip/pull/12))
- *(deps)* lock file maintenance ([#11](https://github.com/so1ve/xwayclip/pull/11))
- *(deps)* lock file maintenance ([#10](https://github.com/so1ve/xwayclip/pull/10))

## [0.2.2](https://github.com/so1ve/xwayclip/compare/v0.2.1...v0.2.2) - 2026-08-22

### Fixed

- avoid thread name > 15 characters

### Other

- publish prebuild binaries

## [0.2.1](https://github.com/so1ve/xwayclip/compare/v0.2.0...v0.2.1) - 2026-08-19

### Added

- make xwayclip a bidirectional utility

### Other

- add .envrc
- remove reductant bin description in cli
- run cargo-sort

## [0.2.0](https://github.com/so1ve/xwayclip/compare/v0.1.2...v0.2.0) - 2026-08-10

### Added

- [**breaking**] remove `initial sync` ability

### Other

- use never type `!` for `run` main loop

## [0.1.2](https://github.com/so1ve/xwayclip/compare/v0.1.1...v0.1.2) - 2026-08-10

### Added

- add nixos home manager module

### Fixed

- avoid exposing clipboard HTML as plain text

### Other

- simplify code
- apply automatic fixes
- display debug logs in debug releases
- update description

## [0.1.1](https://github.com/so1ve/xwayclip/compare/v0.1.0...v0.1.1) - 2026-08-10

### Other

- Configure Renovate ([#1](https://github.com/so1ve/xwayclip/pull/1))
