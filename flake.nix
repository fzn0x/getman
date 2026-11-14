{
  description = "Getman, an open-source Postman alternative written in Rust";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      inherit (nixpkgs) lib;
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      each = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      # Built from source: `nix build`, or `nix run github:fzn0x/getman`.
      packages = each (
        pkgs:
        let
          web = pkgs.callPackage ./nix/web.nix { };
        in
        rec {
          getman = pkgs.callPackage ./nix/package.nix { inherit web; };
          default = getman;
        }
        // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          getman-desktop = pkgs.callPackage ./nix/desktop.nix { inherit web; };
        }
      );

      apps = each (pkgs: rec {
        getman = {
          type = "app";
          program = lib.getExe self.packages.${pkgs.stdenv.hostPlatform.system}.getman;
          meta.description = "Open Getman in your browser";
        };
        default = getman;
      });

      formatter = each (pkgs: pkgs.nixfmt);
    };
}
