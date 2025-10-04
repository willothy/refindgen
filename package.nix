{ lib
, rustPlatform
}:

rustPlatform.buildRustPackage {
  pname = "refindgen";
  version = "0.1.0";
  src = ./.;

  cargoLock.lockFile = ./Cargo.lock;

  meta = with lib; {
    description = "rEFInd bootloader configuration generator for NixOS";
    license = licenses.mit;
    mainProgram = "refindgen";
    platforms = platforms.linux;
  };
}
