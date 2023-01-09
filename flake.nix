{
  description = "mailtest-passwd";

  inputs.nixpkgs.url = "github:nyantec/nixpkgs/release-22.11";

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
        mailtest-passwd = final.callPackage (
          { rustPlatform }:

          rustPlatform.buildRustPackage {
            pname = "mailtest-passwd";
            version =
              self.shortRev or "dirty-${toString self.lastModifiedDate}";
            src = nixpkgs.lib.cleanSourceWith {
              filter = name: type: let
                baseName = baseNameOf (toString name);
              in
                ! (
                  nixpkgs.lib.hasSuffix ".nix" baseName
                );
              src = nixpkgs.lib.cleanSource ./.;
            };
            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            checkInputs = with final; [ postgresql ];
            preCheck = ''
              export PGDATA=$TMP/postgresql-data
              export DATABASE_URL="postgres://localhost?host=$TMP/postgresql&dbname=mail"

              initdb --locale=C --encoding=utf8
              mkdir -p "$TMP/postgresql"
              pg_ctl -o "-c unix_socket_directories=$TMP/postgresql" start
              psql -d postgres -h $TMP/postgresql -c "CREATE DATABASE mail TEMPLATE template0 ENCODING 'utf8' LOCALE 'C';"
            '';
          }
        ) {};
      };

      packages = forAllSystems (system: {
        inherit (nixpkgsFor.${system}) mailtest-passwd;
        default = self.packages.${system}.mailtest-passwd;
      });

      nixosModules.default = import ./configuration.nix self;
      checks = forAllSystems (system: let
        pkgs = (nixpkgsFor.${system});
      in {
        nixos-test = pkgs.nixosTest (import ./nixos-test.nix self);
      });

      devShells = forAllSystems (system: {
        default = nixpkgsFor.${system}.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
          nativeBuildInputs = with nixpkgsFor.${system}; [
            rustfmt sqlx-cli rust-analyzer clippy
          ];

          shellHook = ''
            export DATABASE_URL="postgres://localhost?host=$TMP/postgresql&dbname=mail"
          '';
        };
      });
    };
}
