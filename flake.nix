{
  description = "Low-latency clipboard wrap daemon";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    home-manager.url = "github:nix-community/home-manager/master";
    home-manager.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, flake-utils, home-manager, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        wrap = pkgs.rustPlatform.buildRustPackage {
          pname = "wrap";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
        };
      in
      {
        packages.default = wrap;
        packages.wrap = wrap;

        checks.default = wrap;

        devShells.default = pkgs.mkShell {
          packages = [
            pkgs.cargo
            pkgs.rustc
            pkgs.rustfmt
          ];
        };
      }) // {
        homeManagerModules.default = import ./nix/home-manager.nix self;
        darwinModules.default = {
          homebrew.casks = [ "hammerspoon" ];
        };
      };
}
