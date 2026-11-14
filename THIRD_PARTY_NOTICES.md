# Third-party notices

Getman's own code is MIT OR GPL-3.0-or-later (see [LICENSE-MIT](LICENSE-MIT)
and [LICENSE-GPL](LICENSE-GPL)). These files and libraries of other
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

## Icons

The icons in the browser UI and on the website (`web/src/lib/icons.ts` and
`site/src/icons.mjs`) are **Material Symbols** (Rounded, weight 400),
Copyright Google LLC, under the Apache License, Version 2.0:
<https://www.apache.org/licenses/LICENSE-2.0>. The text of that licence is in
[LICENSES/Apache-2.0.txt](LICENSES/Apache-2.0.txt). Their path data is copied into those files,
which name each symbol; they are not modified.

The Getman logo, the favicon and the desktop app icons (`assets/`,
`web/public/favicon.svg` and `crates/getman-desktop/icons/`) were made for
Getman and are under Getman's licence.

The wordmark and tagline in `assets/logo.svg`, `assets/logo-light.svg` and
`assets/logo-dark.svg` are set in Google Sans Flex (see Fonts above) and
converted to outlines.

## Components

`web/src/components/Orbit.svelte` is a Svelte port of **Orbiting Circles**
from Magic UI (<https://github.com/magicuidesign/magicui>), Copyright (c)
Magic UI, under the MIT License:

> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

## Protocol definitions

`crates/getman-core/src/grpc/rpc/status.proto` and `error_details.proto`
are built into the `getman` binary and the desktop app, which use them to
decode the details of a failed gRPC call. Their message definitions are
those of `google/rpc/status.proto` and `google/rpc/error_details.proto` in
<https://github.com/googleapis/googleapis>, Copyright Google LLC, licensed
under the Apache License, Version 2.0: <https://www.apache.org/licenses/LICENSE-2.0>.
The text of that licence is in [LICENSES/Apache-2.0.txt](LICENSES/Apache-2.0.txt).
Getman rewrote the comments and left out those of the original.

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
