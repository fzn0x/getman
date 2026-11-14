# Third-party notices

Getman's own code is MIT OR Apache-2.0 (see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE)). These files and libraries of other
people are bundled in the `getman` binary, the desktop app or the website,
and keep their own licences.

## Fonts

**Google Sans Flex**, Copyright 2015 The Google Sans Flex Authors
(<https://github.com/googlefonts/googlesans-flex>), and **Google Sans Code**,
Copyright 2025 The Google Sans Code Project Authors
(<https://github.com/googlefonts/googlesans-code>), both under the SIL Open
Font License, Version 1.1.

`web/public/fonts/GoogleSansFlex.woff2` and `GoogleSansCode.woff2` are the
browser UI's text and code faces. They are cut down from the files in
<https://github.com/google/fonts>: Latin characters only, and the weight axis
from 400 to 700 (Google Sans Flex's other axes are fixed at their defaults,
with the optical size at 16). They are built into the browser UI, so they are
in the `getman` binary and the desktop app, and they are served on the website
under `fonts/`. The licence texts are in `OFL-GoogleSansFlex.txt` and
`OFL-GoogleSansCode.txt` beside them, bundled and served with the fonts. The
fonts are not sold on their own.

**Departure Mono**, Copyright 2022-2024 Helena Zhang
(<https://departuremono.com>), under the SIL Open Font License, Version 1.1.

The font file `web/public/fonts/DepartureMono-Regular.woff2` is used by the
website, which takes it from the browser UI's files, so it is also in the
`getman` binary and the desktop app. The licence text is in
`web/public/fonts/OFL-DepartureMono.txt`, which is bundled and served beside
the font. The font is not modified or sold on its own.

## Icons

The icons in the browser UI (`web/src/lib/icons.ts`) are **Material
Symbols** (Rounded, weight 400), Copyright Google LLC, under the Apache
License, Version 2.0: <https://www.apache.org/licenses/LICENSE-2.0>. The text
of that licence is in [LICENSE-APACHE](LICENSE-APACHE). Their path data is
copied into `icons.ts`, which names each symbol; they are not modified.

The icons on the website (`site/src/icons.mjs`), the Getman logo, the favicon
and the desktop app icons (`assets/`, `web/public/favicon.svg` and
`crates/getman-desktop/icons/`) were made for Getman and are under Getman's
licence.

The wordmark and tagline in `assets/logo.svg`, `assets/logo-light.svg` and
`assets/logo-dark.svg` are set in Google Sans Flex (see Fonts above) and
converted to outlines.

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
