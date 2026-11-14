# The getman command, with the browser UI built into it. The UI is built
# first, since crates/getman embeds web/dist as it compiles.
{
  lib,
  rustPlatform,
  web,
  pkg-config,
  zstd,
}:

let
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;
in
rustPlatform.buildRustPackage {
  pname = "getman";
  inherit version;

  src = lib.cleanSource ../.;
  cargoLock.lockFile = ../Cargo.lock;

  # The workspace's default members are this crate and its core; the
  # desktop app needs a webview's libraries and is not part of it.
  cargoBuildFlags = [
    "--package"
    "getman"
  ];

  preBuild = ''
    mkdir -p web
    cp -r --no-preserve=mode ${web} web/dist
  '';

  # zstd-sys, which the zstd crate of getman-core needs, links the system's zstd
  # when it finds it with pkg-config, and else compiles its own C copy.
  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ zstd ];

  # CI runs the tests; some of them start servers or need the network.
  doCheck = false;

  meta = {
    description = "Open-source Postman alternative written in Rust";
    homepage = "https://fzn0x.github.io/getman/";
    license = with lib.licenses; [
      mit
      gpl3Plus
    ];
    mainProgram = "getman";
    platforms = lib.platforms.unix;
  };
}
