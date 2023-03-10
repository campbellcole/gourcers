{
  description = "gource your entire github account";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem(system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in
      with pkgs;
      {
        devShells.default = pkgs.mkShell rec {
          nativeBuildInputs = [
            gource
            pkg-config
            clang
            rust-analyzer
            # git # globally installed, must have SSH keys set up
            (callPackage ./pkgs/qsv {} )
            ffmpeg_5
          ];

          buildInputs = [
            (rust-bin.stable.latest.default.override { extensions = [ "rust-src" ]; })
            openssl
          ];

          LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;
        };
      }
    );
}
