{
  description = "lazyspec - specification management tool";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "clippy" "rustfmt" "rust-src" ];
        };

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
      in
      {
        packages.default = rustPlatform.buildRustPackage {
          pname = "lazyspec";
          version = "0.5.0";

          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              let baseName = builtins.baseNameOf path;
              in !(baseName == "docs" || baseName == "target" || baseName == ".claude");
          };

          cargoHash = "sha256-7ccAvyRnbmXnpmLlyIlv74Fa4cSHtLzkM2LBNqk++tc=";

          # Tests require filesystem fixtures not available in the nix sandbox
          doCheck = false;

          nativeBuildInputs = [ pkgs.pkg-config ];

          buildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.apple-sdk_15
          ];
        };

        checks = {
          clippy = self.packages.${system}.default.overrideAttrs (old: {
            pname = "lazyspec-clippy";
            nativeBuildInputs = (old.nativeBuildInputs or []) ++ [ pkgs.clippy ];
            buildPhase = ''
              cargo clippy -- -D warnings
            '';
            installPhase = ''
              touch $out
            '';
            doCheck = false;
          });

          fmt = self.packages.${system}.default.overrideAttrs (old: {
            pname = "lazyspec-fmt";
            buildPhase = ''
              cargo fmt --check
            '';
            installPhase = ''
              touch $out
            '';
            doCheck = false;
          });

          test = self.packages.${system}.default.overrideAttrs (old: {
            pname = "lazyspec-test";
            nativeBuildInputs = (old.nativeBuildInputs or []) ++ [ pkgs.git ];
            buildPhase = ''
              export HOME=$(mktemp -d)
              git config --global user.email "nix@test"
              git config --global user.name "nix"
              git init
              git add -A
              git commit -m "init" --allow-empty
              cargo test
            '';
            installPhase = ''
              touch $out
            '';
            doCheck = false;
          });
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
          packages = [
            rustToolchain
            pkgs.rust-analyzer
            pkgs.ast-grep
            pkgs.ripgrep
          ];
        };
      }
    );
}
