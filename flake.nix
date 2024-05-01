{
  description = "Sprokkel - A lightweight static site generator";
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
  };
  outputs = { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      rec {
        packages.sprokkel = pkgs.rustPlatform.buildRustPackage {
          pname = "sprokkel";
          version = "0.1.0";
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "tree-sitter-djot-0.0.1" = "sha256-uM9UZRBUIGP66FFuyuSHXLOy0JheDdQUWlFpjsbPpXE=";
            };
          };
        };
        packages.default = packages.sprokkel;
        devShells.default = pkgs.mkShell
          {
            buildInputs = with pkgs; [
              cargo
              clippy
              rust-analyzer
              rustc
              rustfmt
            ];
          };
      }
    );
}
