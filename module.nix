{ config
, lib
, pkgs
, ...
}:
let
  inherit (lib) mkIf mkEnableOption mkOption types;

  cfg = config.boot.loader.refind;
  efi = config.boot.loader.efi;

  # Generate the JSON config that refindgen expects
  refindInstallConfig = pkgs.writeText "refind-install.json" (
    builtins.toJSON {
      nixPath = "${config.nix.package}";
      refindPath = "${cfg.package}";
      efiMountPoint = efi.efiSysMountPoint;
      efiBootMgrPath = "${pkgs.efibootmgr}";
      canTouchEfiVariables = efi.canTouchEfiVariables;
      efiRemovable = cfg.efiInstallAsRemovable;
      timeout = if config.boot.loader.timeout != null then config.boot.loader.timeout else 10;
      maxGenerations = if cfg.maxGenerations == null then 0 else cfg.maxGenerations;
      extraConfig = cfg.extraConfig;
      hostArchitecture = pkgs.stdenv.hostPlatform.system;
      additionalFiles = cfg.additionalFiles;
      luksDevices = lib.mapAttrsToList (name: dev: [ name dev.device ])
        config.boot.initrd.luks.devices;
    }
  );

in
{
  # Extend existing boot.loader.refind options
  options.boot.loader.refind.refindgen = {
    enable = mkEnableOption "refindgen (Rust-based config generator)";
  };

  # Only apply if both refind and refindgen are enabled
  config = mkIf (cfg.enable or false && cfg.refindgen.enable) {
    # Override the install bootloader script to use refindgen
    system.build.installBootLoader = lib.mkForce (
      pkgs.writeScript "install-refind-bootloader" ''
        #!${pkgs.runtimeShell}
        set -e
        export CONFIG_PATH="${refindInstallConfig}"
        exec ${pkgs.refindgen}/bin/refindgen
      ''
    );
  };
}
