# Third-party notices

Getman's own code is MIT OR Apache-2.0 (see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE)). These files and libraries of other
people are bundled in the `getman` binary, the desktop app or the website,
and keep their own licences.

## Font

**Departure Mono**, Copyright 2022-2024 Helena Zhang
(<https://departuremono.com>), under the SIL Open Font License, Version 1.1.

The font file `web/public/fonts/DepartureMono-Regular.woff2` is built into
the browser UI, so it is in the `getman` binary and the desktop app, and it
is served on the website under `fonts/`. The licence text is in
`web/public/fonts/OFL-DepartureMono.txt`, which is bundled and served beside
the font in all three. The font is not modified or sold on its own.

## Icons

The icons in the browser UI, the website, the favicon and the desktop app
(`web/src/lib/icons.ts`, `site/src/icons.mjs`, `web/public/favicon.svg` and
`crates/getman-desktop/icons/`) are pixel drawings made for Getman and are
under Getman's licence. No icon set of anyone else is bundled.

## Protocol definitions

`crates/getman-core/src/grpc/rpc/status.proto` and `error_details.proto`
are built into the `getman` binary and the desktop app, which use them to
decode the details of a failed gRPC call. Their message definitions are
those of `google/rpc/status.proto` and `google/rpc/error_details.proto` in
<https://github.com/googleapis/googleapis>, Copyright Google LLC, licensed
under the Apache License, Version 2.0: <https://www.apache.org/licenses/LICENSE-2.0>.
The text of that licence is in [LICENSE-APACHE](LICENSE-APACHE). Getman
rewrote the comments and left out those of the original.

## Libraries

The browser UI is bundled from npm packages, all under MIT, ISC, Apache-2.0
or BSD licences: CodeMirror, Lezer, GraphQL.js, `graphql-language-service`,
`cm6-graphql` and Tauri's JavaScript API, with the packages they need. The
`getman` binary and the desktop app link Rust crates from crates.io under
permissive licences or, for a few, the Mozilla Public License 2.0. Their
names, versions and licences are in `web/package-lock.json`,
`site/package-lock.json` and `Cargo.lock`, and each package's own licence
file is in its source. Continuous integration checks the licence of every
one against [deny.toml](deny.toml) and
[.github/scripts/check-npm-licenses.mjs](.github/scripts/check-npm-licenses.mjs).
