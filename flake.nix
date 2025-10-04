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
      # Overlay that adds refindgen to pkgs
      overlays.default = final: prev: {
        refindgen = final.callPackage ./package.nix { };
      };

      # Expose packages for each system
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ self.overlays.default ];
          };
        in
        {
          default = pkgs.refindgen;
        }
      );

      # NixOS module that extends the existing refind module
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
