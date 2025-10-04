{ config
, lib
, pkgs
, ...
}:
let
  inherit (lib)
    mkIf
    mkEnableOption
    mkOption
    literalExpression
    types
    ;

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

  # Get refindgen package from the flake
  refindgenPkg = cfg.refindgen.package;

in
{
  options = {
    boot.loader.refind = {
      enable = mkEnableOption "the rEFInd boot loader";

      package = lib.mkPackageOption pkgs "refind" { };

      refindgen = {
        enable = mkEnableOption "refindgen (Rust-based config generator)";

        package = mkOption {
          type = types.package;
          description = "The refindgen package to use for generating configuration";
        };
      };

      extraConfig = mkOption {
        default = "";
        type = types.lines;
        description = ''
          A string which is prepended to refind.conf.
        '';
      };

      maxGenerations = mkOption {
        default = null;
        example = 50;
        type = types.nullOr types.int;
        description = ''
          Maximum number of latest generations in the boot menu.
          Useful to prevent boot partition from running out of disk space.
          `null` means no limit i.e. all generations that were not
          garbage collected yet.
        '';
      };

      additionalFiles = mkOption {
        default = { };
        type = types.attrsOf types.path;
        example = literalExpression ''
          { "icons/os_arch.png" = "''${pkgs.refind}/share/refind/icons/os_arch.png"; }
        '';
        description = ''
          A set of files to be copied to the refind directory in {file}`/boot/efi/refind`.
          Each attribute name denotes the destination file name, while the corresponding
          attribute value specifies the source file.
        '';
      };

      efiInstallAsRemovable = mkEnableOption null // {
        default = !efi.canTouchEfiVariables;
        defaultText = literalExpression "!config.boot.loader.efi.canTouchEfiVariables";
        description = ''
          Whether or not to install the rEFInd EFI files as removable.
          See {option}`boot.loader.grub.efiInstallAsRemovable`
        '';
      };
    };
  };

  config = mkIf (cfg.enable && cfg.refindgen.enable) {
    assertions = [
      {
        assertion =
          pkgs.stdenv.hostPlatform.isx86_64
          || pkgs.stdenv.hostPlatform.isi686
          || pkgs.stdenv.hostPlatform.isAarch64;
        message = "rEFInd can only be installed on aarch64 & x86 platforms";
      }
      {
        assertion = pkgs.stdenv.hostPlatform.isEfi;
        message = "rEFInd can only be installed on UEFI platforms";
      }
    ];

    # Set this loader as active
    system.boot.loader.id = "refind";

    # Install script that calls refindgen with the config file
    system.build.installBootLoader = pkgs.writeScript "install-refind-bootloader" ''
      #!${pkgs.runtimeShell}
      set -e
      export CONFIG_PATH="${refindInstallConfig}"
      exec ${refindgenPkg}/bin/refindgen
    '';
  };
}
