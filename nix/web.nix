# The browser UI, built from web/. The command and the desktop app embed
# the result as they compile.
{
  lib,
  buildNpmPackage,
  importNpmLock,
}:

buildNpmPackage {
  pname = "getman-web";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;
  src = lib.cleanSource ../web;
  # The lockfile names every package's hash, so nothing here is kept by
  # hand: a change of package-lock.json is all `nix build` needs.
  npmDeps = importNpmLock { npmRoot = ../web; };
  npmConfigHook = importNpmLock.npmConfigHook;
  installPhase = ''
    runHook preInstall
    cp -r dist $out
    runHook postInstall
  '';
}
