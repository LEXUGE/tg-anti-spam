{
  description = "Yet another anti-spam bot for Telegram";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    utils.url = "github:numtide/flake-utils";
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      utils,
      git-hooks,
      ...
    }:
    with nixpkgs.lib;
    let
      pkgsWithRust =
        system:
        import nixpkgs {
          system = "${system}";
          overlays = [ rust-overlay.overlays.default ];
          config.allowUnfree = true;
        };
      pkgSet = system: rec {
        default = tg-anti-spam;
        tg-anti-spam =
          with (pkgsWithRust system);
          (makeRustPlatform {
            cargo = rust-bin.stable.latest.default;
            rustc = rust-bin.stable.latest.default;
          }).buildRustPackage
            {
              name = "tg-anti-spam";
              version = "git";
              src = lib.cleanSource ./.;
              cargoLock = {
                lockFile = ./Cargo.lock;
              };
            };
      };
    in
    utils.lib.eachSystem
      (with utils.lib.system; [
        x86_64-linux
      ])
      (system: rec {
        packages = (pkgSet system);

        checks = {
          pre-commit-check = git-hooks.lib.${system}.run {
            src = ./.;
            hooks = {
              nixfmt.enable = true;
              rustfmt.enable = true;
            };
          };
        };

        apps = rec {
          default = tg-anti-spam;
          tg-anti-spam = (
            utils.lib.mkApp {
              drv = packages."tg-anti-spam";
            }
          );
        };

        devShells.default =
          with (pkgsWithRust system);
          mkShell {
            inherit (self.checks.${system}.pre-commit-check) shellHook;
            nativeBuildInputs = [
              # write rustfmt first to ensure we are using nightly rustfmt
              rust-bin.nightly."2026-01-01".rustfmt
              rust-bin.stable.latest.default
              rust-bin.stable.latest.rust-src
              rust-analyzer

              binutils-unwrapped
              cargo-cache
              cargo-outdated

              antigravity
            ];
          };
      })
    // {
      overlays.default = final: prev: (pkgSet prev.pkgs.system);
    };
}
