# The desktop app, the same program as the command: `getman desktop` or the
# app launcher opens the window, any other arguments run the command line.
# Linux only; the app needs WebKitGTK.
{
  lib,
  rustPlatform,
  cargo-tauri,
  pkg-config,
  wrapGAppsHook3,
  webkitgtk_4_1,
  gtk3,
  librsvg,
  glib-networking,
  zstd,
  web,
}:

let
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;
in
rustPlatform.buildRustPackage {
  pname = "getman-desktop";
  inherit version;

  src = lib.cleanSource ../.;
  cargoLock.lockFile = ../Cargo.lock;
  buildAndTestSubdir = "crates/getman-desktop";

  nativeBuildInputs = [
    cargo-tauri.hook
    pkg-config
    wrapGAppsHook3
  ];
  buildInputs = [
    webkitgtk_4_1
    gtk3
    librsvg
    glib-networking
    zstd
  ];

  preBuild = ''
    mkdir -p web
    cp -r --no-preserve=mode ${web} web/dist
  '';

  doCheck = false;

  meta = {
    description = "Getman as a desktop app: a local-first, account-free API client";
    homepage = "https://fzn0x.github.io/getman/";
    license = with lib.licenses; [
      mit
      gpl3Plus
    ];
    mainProgram = "getman";
    platforms = lib.platforms.linux;
  };
}
