{

  description = "mailtest-passwd";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

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
            src = self;
            cargoLock = {
              lockFile = ./Cargo.lock;
            };
          }
        ) {};
      };

      packages = forAllSystems (system: {
        inherit (nixpkgsFor.${system}) mailtest-passwd;
        default = self.packages.${system}.mailtest-passwd;
      });

      devShells = forAllSystems (system: {
        default = nixpkgsFor.${system}.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
          nativeBuildInputs = with nixpkgsFor.${system}; [
            rustfmt
          ];
        };
      });
    };
}
