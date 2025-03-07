{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { system = system; };
      in
      {
        formatter = pkgs.nixfmt-rfc-style;
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustc
            cargo
            clippy
            pkg-config
            openssl
            rustfmt
            rust-analyzer
            xorg.libxcb
          ];
        };

        packages.default = pkgs.rustPlatform.buildRustPackage rec {
          pname = "monitor-affinity";
          version = "0.1.1";
          useFetchCargoVendor = true;

          src = pkgs.fetchFromGitHub {
            owner = "davidmreed";
            repo = pname;
            rev = version;
            hash = "sha256-kzJgrF7sTWOtzIafnpe9NaVD6e60xyCazSfHz7pGf5o=";
          };

          cargoHash = "sha256-uH2jQyxY5xBxGvHvTqiS20pKUlaUroDvxsaeuaKf63M=";
          nativeBuildInputs = with pkgs; [ xorg.libxcb ];
          buildInputs = with pkgs; [ xorg.libxcb ];

          meta = {
            description = "Route bars and widgets to monitors based on criteria like \"largest\" or \"rightmost\".";
            homepage = "https://github.com/davidmreed/monitor-affinity";
            license = pkgs.lib.licenses.mit;
            maintainers = [ ];
          };
        };
      }
    );
}
