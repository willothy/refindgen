{
  description = "rEFInd bootloader configuration generator for NixOS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "refindgen";
          version = "0.1.0";
          src = ./.;

          cargoHash = "sha256-QLSkPnEHxuoKfwuncLQlTBCxpAbzBULQZRrV3nhrFC4=";

          meta = with pkgs.lib; {
            description = "rEFInd bootloader configuration generator for NixOS";
            license = licenses.mit;
            mainProgram = "refindgen";
          };
        };

        # Useful for development
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            rust-analyzer
            rustfmt
            clippy
          ];
        };
      }
    );
}
