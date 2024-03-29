{
  description = "nyanpasswd: A novel authentication system that treats passwords more like per-client tokens";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/release-23.05";

  outputs = { self, nixpkgs }:
  let
    systems = [ "x86_64-linux" ];
    forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
    nixpkgsFor = forAllSystems (system: import nixpkgs {
      inherit system;
      overlays = [ self.overlays.default ];
    });
  in {
    overlays.default = final: prev: {
      mail-passwd = final.nyanpasswd;
      nyanpasswd = final.callPackage (
        { lib, rustPlatform, postgresql }: let
          cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        in rustPlatform.buildRustPackage {
          pname = "nyanpasswd";
          version = cargoToml.package.version;
          src = nixpkgs.lib.cleanSourceWith {
            filter = name: type: let
              baseName = baseNameOf (toString name);
            in
            ! (
              nixpkgs.lib.hasSuffix ".nix" baseName
              || nixpkgs.lib.hasSuffix ".lua" baseName
              || nixpkgs.lib.hasPrefix "radicale-plugin/" name
            );
            src = nixpkgs.lib.cleanSource ./.;
          };
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          
          nativeCheckInputs = [ postgresql ];

          preCheck = ''
            export PGDATA=$TMP/postgresql-data
            export DATABASE_URL="postgres://localhost?host=$TMP/postgresql&dbname=mail"
            
            initdb --locale=C --encoding=utf8
            mkdir -p "$TMP/postgresql"
            pg_ctl -o "-c unix_socket_directories=$TMP/postgresql" start
            psql -d postgres -h $TMP/postgresql -c "CREATE DATABASE mail TEMPLATE template0 ENCODING 'utf8' LOCALE 'C';"
          '';

          meta = {
            maintainers = with lib.maintainers; [ vikanezrimaya ];
          };
        }
      ) {};
      radicale-plugin-mail-passwd = final.radicale-plugin-nyanpasswd;
      radicale-plugin-nyanpasswd = final.callPackage (
        { lib, python3Packages }:
        python3Packages.buildPythonPackage {
          pname = "radicale-plugin-nyanpasswd";
          version = "0.1.0";
          src = ./radicale-plugin;

          propagatedBuildInputs = with python3Packages; [
            requests
          ];

          doCheck = false;

          meta = {
            maintainers = with lib.maintainers; [ vikanezrimaya ];
          };
        }
      ) {};
      radicale-with-mail-passwd = final.radicale-with-nyanpasswd;
      radicale-with-nyanpasswd = prev.radicale.overrideAttrs (old: {
        propagatedBuildInputs = old.propagatedBuildInputs ++ [
          final.radicale-plugin-nyanpasswd
        ];
      });
    };

    packages = forAllSystems (system: {
      inherit (nixpkgsFor.${system})
        mail-passwd radicale-plugin-mail-passwd
        nyanpasswd radicale-plugin-nyanpasswd;
      default = self.packages.${system}.mail-passwd;
    });

    nixosModules.default = import ./configuration.nix self;
    checks = forAllSystems (system: let
      pkgs = (nixpkgsFor.${system});
    in {
      nixos-test = nixpkgs.lib.nixos.runTest (import ./nixos-test.nix self pkgs);
    });

    devShells = forAllSystems (system: {
      default = nixpkgsFor.${system}.mkShell {
        inputsFrom = [ self.packages.${system}.default ];
        nativeBuildInputs = with nixpkgsFor.${system}; [
          rustfmt sqlx-cli rust-analyzer clippy cargo-watch postgresql
        ];

        shellHook = ''
          export DATABASE_URL="postgres://localhost?host=$TMP/postgresql&dbname=mail"
        '';
      };
    });
  };
}
