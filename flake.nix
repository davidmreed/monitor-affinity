{
  inputs = {
    nixpkgs.url = github:nixos/nixpkgs/nixpkgs-unstable;
    flake-utils.url = github:numtide/flake-utils;
  };

  outputs = { self, nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = import nixpkgs { system = system; }; in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [ rustc cargo clippy pkg-config openssl rustfmt rust-analyzer xorg.libxcb ];
        };

        packages.default = pkgs.rustPlatform.buildRustPackage rec {
            pname = "monitor-affinity";
            version = "0.1.0";

            src = pkgs.fetchFromGitHub {
              owner = "davidmreed";
              repo = pname;
              rev = version;
              hash = "sha256-aCVRTqIb1Kf7DFDBBz+bM8aAcOg1k+tPzCvW5YAYK8E=";
            };

            cargoHash = "sha256-HTZ56KZFmG5qKbn/vvDbXVKvf10dqY6dpNqA/Gm8bXg=";

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
