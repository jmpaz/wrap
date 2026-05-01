self:
{ config, lib, pkgs, ... }:

let
  cfg = config.programs.wrap;
  package = cfg.package;
in
{
  options.programs.wrap = {
    enable = lib.mkEnableOption "low-latency clipboard wrap daemon";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      description = "Package providing wrap, wrapctl, and wrapd.";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ package ];

    systemd.user.sockets.wrapd = {
      Unit.Description = "Socket for wrapd clipboard daemon";
      Socket = {
        ListenStream = "%t/wrap/wrapd.sock";
        SocketMode = "0600";
        DirectoryMode = "0700";
      };
      Install.WantedBy = [ "sockets.target" ];
    };

    systemd.user.services.wrapd = {
      Unit = {
        Description = "Low-latency clipboard wrap daemon";
        Requires = [ "wrapd.socket" ];
        After = [ "graphical-session.target" "wrapd.socket" ];
        PartOf = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = "${package}/bin/wrapd";
        Restart = "on-failure";
        RestartSec = 1;
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
