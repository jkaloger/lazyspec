{
  description = "lazyspec - specification management tool";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, crane, flake-utils, git-hooks, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        craneLib = crane.mkLib pkgs;
        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            (craneLib.filterCargoSources path type)
            || (pkgs.lib.hasSuffix ".md" path);
        };

        commonArgs = {
          inherit src;
          strictDeps = true;
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.apple-sdk_15
          ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        pre-commit-check = git-hooks.lib.${system}.run {
          src = ./.;
          hooks = {
            rustfmt.enable = true;
            clippy = {
              enable = true;
              stages = [ "pre-push" ];
              settings = {
                denyWarnings = true;
                allFeatures = false;
                offline = true;
                extraArgs = "--all-targets";
              };
            };
          };
        };
      in
      {
        packages.default = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          doCheck = false;
        });

        checks = {
          pre-commit = pre-commit-check;

          clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

          fmt = craneLib.cargoFmt {
            inherit src;
          };

          test = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
            nativeBuildInputs = commonArgs.nativeBuildInputs ++ [ pkgs.git ];
            preCheck = ''
              export HOME=$(mktemp -d)
              git config --global user.email "nix@test"
              git config --global user.name "nix"
              git init
              git add -A
              git commit -m "init" --allow-empty
            '';
          });
        };

        formatter = pkgs.nixfmt;

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};
          # lld lives here (not .cargo/config.toml) so CI builds with plain
          # Apple clang, which has no lld, still link.
          CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS = "-C link-arg=-fuse-ld=lld";
          shellHook = pre-commit-check.shellHook;
          packages = pre-commit-check.enabledPackages ++ [
            pkgs.clippy
            pkgs.rustfmt
            pkgs.rust-analyzer
            pkgs.ast-grep
            pkgs.ripgrep
            pkgs.cargo-sweep
            pkgs.llvmPackages.lld
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.cargo-tauri
          ];
        };
      }
    );
}
