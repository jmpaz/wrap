self:
{ config, lib, pkgs, ... }:

let
  cfg = config.programs.wrap;
  package = cfg.package;
  isLinux = pkgs.stdenv.isLinux;
  isDarwin = pkgs.stdenv.isDarwin;
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

    systemd.user.sockets.wrapd = lib.mkIf isLinux {
      Unit.Description = "Socket for wrapd clipboard daemon";
      Socket = {
        ListenStream = "%t/wrap/wrapd.sock";
        SocketMode = "0600";
        DirectoryMode = "0700";
      };
      Install.WantedBy = [ "sockets.target" ];
    };

    systemd.user.services.wrapd = lib.mkIf isLinux {
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

    home.file.".hammerspoon/init.lua" = lib.mkIf isDarwin {
      text = ''
        local eventtap = require("hs.eventtap")
        local hotkey = require("hs.hotkey")
        local task = require("hs.task")
        local urlevent = require("hs.urlevent")

        local wrapctl = "${package}/bin/wrapctl"

        local function run(args)
          task.new(wrapctl, nil, args):start()
        end

        urlevent.bind("wrap-paste", function()
          eventtap.keyStroke({ "cmd" }, "v", 0)
        end)

        hotkey.bind({}, "f13", function() run({ "paste", "md" }) end)
        hotkey.bind({}, "f14", function() run({ "paste", "xml" }) end)
        hotkey.bind({}, "f15", function() run({ "unwrap-paste" }) end)
      '';
    };

    launchd.agents.wrap-hammerspoon = lib.mkIf isDarwin {
      enable = true;
      config = {
        ProgramArguments = [ "/usr/bin/open" "-gja" "Hammerspoon" ];
        RunAtLoad = true;
        ProcessType = "Interactive";
      };
    };
  };
}
