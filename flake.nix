{
  inputs = {
    nixpkgs.url = github:nixos/nixpkgs/nixpkgs-unstable;
    flake-utils.url = github:numtide/flake-utils;
  };

  outputs = { self, nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = import nixpkgs { system = system; config.allowUnfree = true; }; in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [ rustc cargo pkg-config openssl rustfmt rust-analyzer xorg.libxcb ];
        };
      }
    );
}
