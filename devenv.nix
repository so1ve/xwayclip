{ pkgs, ... }:

{
  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
  };

  packages = with pkgs; [
    actionlint
    nixfmt-tree
    pkg-config
    tombi
  ];
}
