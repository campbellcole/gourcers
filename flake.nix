{
  description = "";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    use-mold.url = "github:campbellcole/use-mold";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, use-mold }:
    flake-utils.lib.eachDefaultSystem(system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        moldHook = use-mold.useMoldHook {} pkgs.mold;
      in
      with pkgs;
      {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [
            clang
            gource
            ffmpeg
            (rust-bin.stable.latest.default.override {
              extensions = [ "rust-src" "clippy" "cargo" "rust-analyzer" ];
            })
          ];

          shellHook = moldHook;

          RUST_BACKTRACE = 1;
        };
      }
    );
}
