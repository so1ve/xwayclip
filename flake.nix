{
  description = "Clipboard synchronization from X11 to Wayland";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      cargoToml = fromTOML (builtins.readFile ./Cargo.toml);
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      mkPackage =
        pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          inherit (cargoToml.package) version;

          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.lock
              ./Cargo.toml
              ./src
            ];
          };

          cargoLock.lockFile = ./Cargo.lock;

          meta = {
            inherit (cargoToml.package) description;
            homepage = "https://github.com/so1ve/xwayclip";
            license = pkgs.lib.licenses.mit;
            mainProgram = "xwayclip";
            platforms = pkgs.lib.platforms.linux;
          };
        };
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          xwayclip = mkPackage pkgs;
        in
        {
          inherit xwayclip;
          default = xwayclip;
        }
      );

      apps = forAllSystems (
        system:
        let
          app = {
            type = "app";
            program = nixpkgs.lib.getExe self.packages.${system}.xwayclip;
            meta.description = cargoToml.package.description;
          };
        in
        {
          xwayclip = app;
          default = app;
        }
      );

      overlays.default = final: _prev: {
        xwayclip = mkPackage final;
      };
    };
}
