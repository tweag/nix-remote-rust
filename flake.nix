{
  description = "A basic rust devshell";

  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
    unstable.url = "nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, unstable, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        unstablePkgs = import unstable {
          localSystem = { inherit system; };
        };
      in
      with pkgs;
      {
        devShells.default = mkShell {
          buildInputs = [
            unstablePkgs.rust-analyzer
            rust-bin.nightly.latest.default
            #rust-bin.stable.latest.default
            cargo-expand
          ];
          
          shellHook = ''
            alias ls=exa
            alias find=fd
          '';
        };
      }
    );
}