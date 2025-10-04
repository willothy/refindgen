{
  description = "rEFInd bootloader configuration generator for NixOS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "refindgen";
            version = "0.1.0";
            src = ./.;

            cargoLock.lockFile = ./Cargo.lock;

            meta = with pkgs.lib; {
              description = "rEFInd bootloader configuration generator for NixOS";
              license = licenses.mit;
              mainProgram = "refindgen";
              platforms = platforms.linux;
            };
          };
        }
      );

      nixosModules.default = import ./module.nix;

      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              rust-analyzer
              rustfmt
              clippy
            ];
          };
        }
      );
    };
}
