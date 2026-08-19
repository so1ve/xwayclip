self:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.xwayclip;
in
{
  options.services.xwayclip = {
    enable = lib.mkEnableOption "bidirectional X11 and Wayland clipboard synchronization";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      description = "The xwayclip package to use.";
    };

    extraArgs = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      example = [
        "--transfer-timeout-ms"
        "10000"
      ];
      description = "Additional command-line arguments passed to xwayclip.";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];

    systemd.user.services.xwayclip = {
      Unit = {
        Description = "Synchronize the clipboard between X11 and Wayland";
        After = [ "graphical-session.target" ];
        PartOf = [ "graphical-session.target" ];
      };

      Service = {
        ExecStart = lib.escapeShellArgs ([ (lib.getExe cfg.package) ] ++ cfg.extraArgs);
        Restart = "on-failure";
        RestartSec = 3;
      };

      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
