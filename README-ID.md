<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg">
    <img src="assets/logo.svg" width="480" alt="Getman">
  </picture>
</p>

<p align="center">
  Alternatif Postman yang open source, ditulis dengan Rust.<br>
  Request adalah file YAML di repository kamu, dikirim dari aplikasi desktop, browser, terminal, atau CI.
</p>

<p align="center">
  <a href="https://github.com/fzn0x/getman/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/fzn0x/getman/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/fzn0x/getman/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/fzn0x/getman?color=3f51b5"></a>
  <a href="#lisensi"><img alt="MIT OR GPL-3.0-or-later" src="https://img.shields.io/badge/license-MIT%20OR%20GPL--3.0--or--later-3f51b5.svg"></a>
  <a href="https://fzn0x.github.io/getman/"><img alt="Documentation" src="https://img.shields.io/badge/docs-fzn0x.github.io%2Fgetman-3f51b5.svg"></a>
</p>

---

<p align="center">
  <a href="https://fzn0x.github.io/getman/">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="docs/images/getman-dark.png">
      <img src="docs/images/getman-light.png" width="100%" alt="Getman menampilkan empat check sebuah request di samping response-nya: 200 OK berwarna hijau, dengan waktu, ukuran, dan 4 dari 4 tes lulus">
    </picture>
  </a>
</p>

Getman adalah API client untuk HTTP/1.1, HTTP/2, HTTP/3, gRPC, WebSocket, Server-Sent Events dan Socket.IO. Sebuah workspace adalah folder berisi file YAML, satu per request, yang kamu commit dan review bersama kode yang diujinya. Satu mesin yang sama menjalankan aplikasi desktop, UI browser, UI terminal, dan `getman run`, jadi sebuah request berperilaku sama di mana pun. Getman bisa meng-import Postman collection dan menjalankan script `pm`.

[Install](#install) · [Dokumentasi](https://fzn0x.github.io/getman/) ·
[Command line](docs/cli.md) · [Pindah dari Postman](docs/why-getman.md#moving-over) ·
[Changelog](CHANGELOG.md) · [English](README.md)

## Mulai cepat

```sh
mkdir my-api && cd my-api
getman init --example                         # getman.yaml, 2 environment, 3 request
getman send posts/get-a-post.yaml -e public   # kirim satu request, cetak body-nya
getman run -e public                          # jalankan semua request dan buat laporan
getman                                        # buka UI browser
getman tui                                    # atau UI terminal
```

`--example` menambahkan request ke [JSONPlaceholder](https://jsonplaceholder.typicode.com), API uji coba yang gratis. Lihat [Memulai](docs/getting-started-ID.md).

## Install

### Aplikasi desktop

Aplikasi ini membuka Getman di jendelanya sendiri dan ikut meng-install perintah `getman`.

| Sistem | Unduh |
| --- | --- |
| Windows x64 | [Setup](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-x64-setup.exe) · [MSI](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-x64.msi) · [Portable zip](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-x64-portable.zip) |
| Windows ARM64 | [Setup](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-arm64-setup.exe) · [MSI](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-arm64.msi) · [Portable zip](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-arm64-portable.zip) |
| macOS (Apple silicon dan Intel) | [Disk image](https://github.com/fzn0x/getman/releases/latest/download/Getman-macos-universal.dmg) |
| Linux x86_64 | [AppImage](https://github.com/fzn0x/getman/releases/latest/download/Getman-linux-x86_64.AppImage) · [.deb](https://github.com/fzn0x/getman/releases/latest/download/getman-desktop_amd64.deb) · [.rpm](https://github.com/fzn0x/getman/releases/latest/download/getman-desktop.x86_64.rpm) |
| Linux ARM64 | [AppImage](https://github.com/fzn0x/getman/releases/latest/download/Getman-linux-aarch64.AppImage) · [.deb](https://github.com/fzn0x/getman/releases/latest/download/getman-desktop_arm64.deb) · [.rpm](https://github.com/fzn0x/getman/releases/latest/download/getman-desktop.aarch64.rpm) |

- **Windows:** jalankan setup-nya. Instalasi berlaku untuk user kamu, tanpa hak administrator, dan menambahkan `getman` ke `PATH` kamu. Kalau Windows menampilkan "Windows protected your PC", pilih **More info**, lalu **Run anyway**.
- **macOS:** buka disk image dan seret Getman ke Applications. Saat pertama kali, macOS tidak bisa memverifikasinya: buka **System Settings › Privacy & Security** dan klik **Open Anyway**. **Settings › About** di dalam aplikasi akan meng-install perintah `getman`.
- **Linux:** `chmod +x` AppImage lalu jalankan, atau install paketnya dengan `sudo apt install ./getman-desktop_amd64.deb` atau `sudo dnf install ./getman-desktop.x86_64.rpm`, yang juga menaruh `getman` di `/usr/bin`.

Aplikasi memperbarui dirinya sendiri dari rilis yang ditandatangani. Pengecekan dilakukan sekali sehari, dan kamu bisa mematikannya. Lihat [Aplikasi desktop](docs/desktop.md).

### Command line

```sh
# Linux dan macOS
curl -fsSL https://fzn0x.github.io/getman/install.sh | sh
```

```powershell
# Windows (PowerShell)
irm https://fzn0x.github.io/getman/install.ps1 | iex
```

Script tersebut mengunduh rilis terbaru untuk sistem kamu, mencocokkannya dengan `SHA256SUMS` milik rilis itu, lalu meng-install `getman` untuk user kamu: di `~/.local/bin` pada Linux dan macOS, dan di `%LOCALAPPDATA%\Programs\getman` pada Windows. Atur `GETMAN_VERSION` (misalnya `v0.3.4`) untuk meng-install rilis tertentu, atau `GETMAN_INSTALL_DIR` untuk memilih direktorinya.

Dengan package manager:

```sh
brew install fzn0x/tap/getman          # macOS dan Linux; brew upgrade getman
```

```powershell
scoop bucket add fzn0x https://github.com/fzn0x/scoop-bucket
scoop install fzn0x/getman             # Windows; scoop update getman
```

Archive untuk semua sistem ada di [halaman rilis](https://github.com/fzn0x/getman/releases). Untuk build Getman sendiri, lihat [Pengembangan](#pengembangan).

## Penggunaan

### Workspace

```text
api/
  getman.yaml               pengaturan untuk semua request
  getman.local.yaml         override dan globals milik kamu (diabaikan git)
  environments/
    staging.yaml            nilai yang dipakai bersama
    staging.local.yaml      secret dan token kamu (diabaikan git)
  orders/
    _folder.yaml            auth, header, dan script untuk folder ini
    create-order.yaml
```

```yaml
# api/orders/create-order.yaml
name: Create order
method: POST
url: "{{baseUrl}}/orders"
body:
  type: json
  content: |
    { "sku": "BOOK-42", "quantity": 1 }
checks:
  - status == 201
  - json.id exists
  - time < 500
captures:
  orderId: json.id
```

Setiap perintah mencari workspace dengan melihat `getman.yaml` di direktori saat ini dan direktori induknya; `-w DIR` menunjuk ke tempat lain. Field yang memakai nilai default tidak ditulis, jadi diff tetap kecil. Folder dan workspace meneruskan header, authentication, variable, script, dan pengaturan ke request di dalamnya. History, run sebelumnya, dan cookie disimpan di `.getman/`, yang dicantumkan `getman init` di `.gitignore` bersama file `*.local.yaml`. Lihat [Workspaces](docs/workspaces.md).

### Check dan capture

Sebuah check terdiri dari target, operator, dan biasanya sebuah nilai. Check yang gagal membuat request-nya gagal, dan `getman run` keluar dengan kode 1.

```yaml
checks:
  - status in 200, 201, 204
  - header.content-type contains json
  - json.data[0].email matches ^[^@]+@example\.com$
  - json.data is not empty
  - time between 1 and 500
captures:
  firstUserId: json.data[0].id
  etag: header.etag
```

Target-nya adalah `status`, `time`, `size`, `url`, `body`, `header.<name>`, `cookie.<name>` dan `json.<path>`, ditambah `messages` untuk WebSocket, SSE dan Socket.IO. Operatornya antara lain `==`, `<`, `contains`, `startsWith`, `matches`, `exists`, `is <type>`, `in` dan `between`. Lihat [Check dan capture](docs/checks.md).

### Variable

`{{name}}` bisa dipakai di URL, header, parameter, body, dan field authentication. Kalau dua scope mendefinisikan nama yang sama, scope yang lebih kuat yang menang. Dari yang paling lemah sampai paling kuat: globals, workspace, environment, folder, request, baris data sebuah run, session, local. Variable yang tidak didefinisikan dikirim apa adanya dan dicantumkan di tampilan response. Ada lebih dari 30 nilai dinamis bawaan seperti `{{$uuid}}` dan `{{$timestamp}}`, dan `{{$env.GETMAN_API_TOKEN}}` membaca environment variable yang namanya diawali `GETMAN_`. Lihat [Variable dan environment](docs/variables.md).

### Script

Script berjalan sebelum request dan setelah response-nya datang, di mesin QuickJS yang tertanam. Script tidak butuh Node.js dan tidak bisa membaca file apa pun selain modul `.js` milik workspace itu sendiri.

```yaml
scripts:
  pre_request: |
    gm.request.headers.upsert({ key: "X-Request-Id", value: crypto.randomUUID() });
  post_response: |
    gm.test("returns the order", () => {
      gm.response.to.have.status(200);
      gm.expect(gm.response.json().items).to.be.an("array").that.is.not.empty;
    });
    gm.environment.set("orderId", gm.response.json().id);
```

`pm` adalah objek yang sama dengan `gm`, dan global lama Postman seperti `tests[...]`, `responseBody` dan `postman.setNextRequest` juga tersedia, jadi sebagian besar script Postman jalan tanpa diubah. `bru`, `req` dan `res` dari Bruno juga bisa dipakai. Lihat [Script](docs/scripts.md).

### Perintah

```text
getman [DIR]                   open the browser UI (the default)
getman tui [DIR]               open the terminal UI
getman run [TARGET]...         run folders, requests or the workspace, for CI
getman send REQUEST            send one request and print the body, or
                               connect to a WebSocket, SSE or Socket.IO one
getman init [DIR]              create a workspace
getman import FILE|DIR         import collections, specs, HAR files or cURL
getman export [ITEM] --to X    a request as code, a folder as Postman or OpenAPI
getman mock [FOLDER]           serve saved examples as a mock API
getman mcp [DIR]               let AI agents use the workspace
```

`getman help COMMAND` menampilkan semua opsi. `getman send -i` menambahkan status line, header, timing, dan detail TLS, dan `--json` mencetak request, response, tes, dan log sebagai JSON untuk tool seperti `jq`. Kode keluarnya 0 saat berhasil, 1 saat sebuah request gagal, dan 2 saat perintah tidak bisa menjalankan tugasnya. Output berwarna di terminal; `NO_COLOR=1` mematikan warna dan `CLICOLOR_FORCE=1` mempertahankannya di log CI. Lihat [Command line](docs/cli.md).

### Menjalankan di CI

```yaml
# .github/workflows/api.yml
- name: Install Getman
  run: |
    curl -fsSL https://fzn0x.github.io/getman/install.sh | sh
    echo "$HOME/.local/bin" >> "$GITHUB_PATH"
- name: Run the API checks
  run: getman run -w api -e staging -r cli -r junit=getman.xml
  env:
    GETMAN_API_TOKEN: ${{ secrets.API_TOKEN }}
```

```text
Orders API with the staging environment: 3 requests

  ✓ POST    Create order   201 Created 182 ms · 96 B
  ✗ GET     Get order      404 Not Found 41 ms · 23 B
      ✗ status == 200
        status is 404; expected == 200
  ✓ DELETE  Archive order  204 No Content 37 ms · 0 B
```

Reporter-nya adalah `cli`, `json`, `junit`, `html`, `csv` dan `xlsx`. Sebuah run bisa memakai iterasi (`-n`), file data CSV atau JSON (`-d`), `--delay` dan `--bail`. Lihat [Menjalankan di CI](docs/runner.md).

### Import dan export

```sh
getman import orders.postman_collection.json    # Postman, Insomnia atau Hoppscotch
getman import openapi.yaml                      # OpenAPI 3 atau Swagger 2
getman import ./my-bruno-collection             # folder Bruno
getman import --curl 'curl https://api.example.com/users'
getman export orders/create-order.yaml --to python --keep-vars
getman export orders --to openapi -o openapi.yaml
```

Format dikenali dari isinya. Sebuah collection menjadi folder di workspace, dan nilai yang ditandai secret masuk ke file lokal yang diabaikan git. `--to` menulis kode untuk 18 target, dari `curl` dan `fetch` sampai `rust` dan `dart`. `--keep-vars` mempertahankan `{{variables}}` di dalam kode, jadi tidak ada token yang bocor ke dalam snippet. Lihat [Import dan export](docs/import-export.md).

### Mock dan AI agent

`getman mock` menyajikan response yang tersimpan sebagai contoh pada request kamu sebagai mock API di `127.0.0.1`, port 7979 secara default. `getman mcp` adalah server [Model Context Protocol](https://modelcontextprotocol.io) lewat standard input dan output. Lewat server ini, sebuah agent bisa melihat daftar, membaca, mengirim, dan menulis request, dan nilai lokal disamarkan:

```sh
claude mcp add --transport stdio getman -- getman mcp
```

Lihat [Contoh dan mock](docs/examples-and-mocks.md) dan [AI agent](docs/mcp.md).

## Fitur

**Protokol**

- **HTTP/1.1, HTTP/2 dan HTTP/3.** Pilih versinya per request atau per folder, atau biarkan Getman menegosiasikannya; ada petunjuk saat server menawarkan HTTP/3. Body JSON, XML, teks, form, multipart dengan file, binary, dan GraphQL. Lihat [HTTP dan koneksi](docs/http.md).
- **gRPC.** Panggilan unary, server streaming, client streaming, dan bidirectional streaming, dengan skema dari file `.proto` di workspace kamu atau dari server reflection, metadata, deadline, TLS, detail status yang ter-decode, dan satu request per method yang dibuat dari sebuah service. Lihat [gRPC](docs/grpc.md).
- **WebSocket, Server-Sent Events dan Socket.IO.** Terhubung, kirim pesan yang tersimpan atau yang disusun, dan pantau log live yang bisa dicari. Lihat [WebSocket, SSE dan Socket.IO](docs/realtime.md).

**Authentication**

- Basic, Bearer, API key, Digest (MD5, SHA-256, SHA-512-256), OAuth 2.0 (authorization code dengan PKCE, implicit, client credentials, password, refresh token, sign-in di browser kamu), OAuth 1.0a, AWS Signature Version 4, JWT, Atlassian ASAP, Hawk dan Akamai EdgeGrid, diwariskan dari folder. Lihat [Authentication](docs/auth.md).

**Koneksi**

- Proxy (HTTP, HTTPS, SOCKS5), client certificate (PEM atau PKCS#12), CA certificate tambahan, dan cookie jar yang tersimpan antar run dan bisa kamu edit.
- Rincian waktu untuk tiap response: DNS, connect dan TLS, first byte dan download, beserta versi TLS dan sertifikat server.
- Batalkan request apa pun selagi masih berjalan.

**Aplikasi**

- **Aplikasi desktop** untuk Windows, macOS dan Linux, di x64 dan ARM64.
- **UI browser** yang mengutamakan keyboard: tab dan draft yang bertahan setelah restart, command palette (<kbd>Ctrl</kbd> <kbd>K</kbd>), kirim dengan <kbd>Ctrl</kbd> <kbd>Enter</kbd>, semua shortcut di <kbd>?</kbd>, `{{variables}}` yang di-highlight dan dilengkapi dengan nilainya saat di-hover, filter JSON path dan hex view pada response, history yang menyimpan tiap response, trash dengan undo, dan tema terang dan gelap. Lihat [Browser UI](docs/browser-ui.md).
- **UI terminal** untuk sesi SSH, container, dan pane tmux. Header, body, dan auth dibuka di `$EDITOR` kamu. Lihat [Terminal UI](docs/terminal.md).
- **Runner** di UI, dengan iterasi dan file data, yang menyimpan 20 run terakhir.

## Dibandingkan dengan Postman

- **Penyimpanan.** Request, environment, dan history adalah file di dalam workspace. Menyimpan request dan memakai environment bisa dilakukan tanpa akun; Postman mengharuskan kamu login ke sebuah workspace untuk keduanya.
- **Berbagi.** Workspace dibagikan dengan cara me-commit-nya, jadi tidak ada biaya per pengguna.
- **Kompatibilitas.** `getman import` membaca Postman collection (v2.0 dan v2.1), environment, dan globals, `getman export --to postman` menuliskannya kembali, dan script `pm` jalan apa adanya.
- **Belum ada di Getman:** MQTT, hosted mock dan monitor, dan kolaborasi real-time.

[Kenapa Getman](docs/why-getman.md) menautkan halaman Postman sendiri untuk setiap klaim dan menjelaskan kapan sebaiknya tetap di Postman. [API client lain](docs/compare.md) membandingkan Getman dengan Yaak, Hoppscotch, Bruno, dan Beam.

## Performa

Diukur di Windows 11 x64 dengan file rilis 0.3.0, pada 28 Oktober 2026:

| | |
| --- | --- |
| Perintah `getman`, UI browser termasuk di dalamnya | 9,5 MiB untuk diunduh, 25,6 MiB di disk |
| Waktu start-up `getman` | 7 ms |
| Server UI browser, saat idle | 2 MiB memori |
| Setup aplikasi desktop, perintah `getman` termasuk | 15,2 MiB |
| Aplikasi desktop, dari launch sampai jendela termuat | 0,45 detik, 0,8 detik untuk pertama kali |
| Aplikasi desktop, saat idle | 94 MiB memori: 5 MiB adalah Getman, sisanya WebView2 milik sistem |

Memori adalah private working set, yang ditampilkan Task Manager. Angkanya berbeda-beda tergantung sistem dan workspace.

## Keamanan

UI browser disajikan oleh Getman hanya di `127.0.0.1`. Server menolak request yang ditujukan ke host name lain (DNS rebinding) atau yang dikirim dari halaman web lain, dan setiap panggilan API butuh token acak yang hanya dibawa oleh link dari terminal kamu. Request hanya bisa meng-upload file dari dalam workspace, dan `{{$env.…}}` hanya membaca variable yang diawali `GETMAN_`. Pengecekan update harian aplikasi desktop adalah satu-satunya request yang dibuat Getman atas inisiatifnya sendiri, dan kamu bisa mematikannya. Lihat [Keamanan dan privasi](docs/security.md), dan [SECURITY.md](SECURITY.md) untuk melaporkan kerentanan.

## Dokumentasi

- [Memulai](docs/getting-started-ID.md) ·
  [Aplikasi desktop](docs/desktop.md) ·
  [Kenapa Getman](docs/why-getman.md), dan pindah dari Postman ·
  [API client lain](docs/compare.md)
- [Workspaces](docs/workspaces.md) ·
  [Variable dan environment](docs/variables.md) ·
  [Authentication](docs/auth.md) ·
  [Check dan capture](docs/checks.md) · [Script](docs/scripts.md)
- [HTTP dan koneksi](docs/http.md) ·
  [WebSocket, SSE dan Socket.IO](docs/realtime.md) · [gRPC](docs/grpc.md)
- [Menjalankan di CI](docs/runner.md) ·
  [Contoh dan mock](docs/examples-and-mocks.md) ·
  [AI agent](docs/mcp.md) · [Import dan export](docs/import-export.md)
- [Browser UI](docs/browser-ui.md) · [Terminal UI](docs/terminal.md) ·
  [Command line](docs/cli.md) · [Keamanan dan privasi](docs/security.md)

## Pengembangan

### Build dari source

Kamu butuh [Rust](https://rustup.rs) 1.88 atau lebih baru dan [Node.js](https://nodejs.org) 22.12 atau lebih baru. UI browser di-build dulu, karena tertanam di dalam binary.

```sh
git clone https://github.com/fzn0x/getman.git
cd getman/web
npm ci
npm run build
cd ..
cargo install --path crates/getman
```

Aplikasi desktop butuh Rust 1.90 atau lebih baru dan, di Linux, WebKitGTK; jalankan dengan `cargo run -p getman-desktop`. `cargo build` dan `cargo test` biasa tidak menyertakannya, dan `--workspace` menyertakannya.

### Mengerjakan UI browser

Build debug menyajikan `web/dist` langsung dari disk, jadi `npm run build` lalu reload sudah cukup untuk melihat perubahan. Untuk hot reload, jalankan Getman di port default-nya dan Vite di sampingnya:

```sh
cargo run -- init --example /tmp/play
cargo run -- /tmp/play --no-open      # mencetak link dengan #token=...
cd web && npm run dev                 # lalu buka link itu di port 5173
```

Atur `GETMAN_URL` kalau Getman berjalan di port selain 7878.

### Sebelum membuat pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- --deny warnings
cargo test --workspace
cd web && npm run format && npm run check && npm test
```

Clippy berjalan dengan lint pedantic-nya, dan crate-nya melarang kode unsafe. CI menjalankan semua ini di Linux, Windows dan macOS. CI juga melakukan smoke test pada command line, memeriksa versi Rust minimum yang didukung, menjalankan snippet kode hasil export di bahasanya masing-masing, dan memeriksa lisensi dan advisory setiap dependency. [CONTRIBUTING.md](CONTRIBUTING.md) menjelaskan sisanya, termasuk installer-nya.

### Arsitektur

| Path | Isi |
| --- | --- |
| `crates/getman-core` | Mesinnya: file workspace, variable, HTTP, gRPC, WebSocket, SSE dan Socket.IO, auth, check, script, runner, import dan export |
| `crates/getman` | Perintah `getman`: command line, server lokal, UI terminal, reporter, mock server dan MCP server |
| `crates/getman-desktop` | Aplikasi desktop, dengan Tauri 2, dan update-nya |
| `web` | UI browser: Svelte 5, TypeScript dan CodeMirror 6 |
| `site` | Website dokumentasi, diterbitkan ke GitHub Pages |
| `docs` | Halaman-halaman dokumentasi |

`getman-core` mengirim HTTP/1.1 dan HTTP/2 dengan reqwest dan hyper, dan HTTP/3 lewat QUIC dengan quinn dan h3. TLS-nya rustls, yang memeriksa sertifikat terhadap trust store sistem. gRPC adalah client buatan Getman sendiri di atas HTTP/2 milik hyper: protox meng-compile file `.proto` saat runtime dan prost-reflect meng-encode pesannya, jadi tidak ada kode yang di-generate lebih dulu. WebSocket berjalan di atas tokio-tungstenite, dan Server-Sent Events serta Socket.IO diimplementasikan di dalam crate itu. Script berjalan di QuickJS-ng lewat rquickjs. Crate `getman` menyajikan UI browser dengan axum, dari file yang ditanam saat build dan disimpan dalam kompresi Brotli, dan menggambar UI terminal dengan Ratatui. Aplikasi desktop menjalankan server yang sama dan menampilkannya di webview sistem.

## Kredit

Getman dibangun di atas proyek-proyek open source ini, dan masih banyak lagi:

- **Rust:** [Tokio](https://tokio.rs), [hyper](https://hyper.rs),
  [reqwest](https://github.com/seanmonstar/reqwest),
  [rustls](https://github.com/rustls/rustls),
  [quinn](https://github.com/quinn-rs/quinn) dan
  [h3](https://github.com/hyperium/h3),
  [prost](https://github.com/tokio-rs/prost),
  [prost-reflect dan protox](https://github.com/andrewhickman/prost-reflect),
  [tokio-tungstenite](https://github.com/snapview/tokio-tungstenite),
  [rquickjs](https://github.com/DelSkayn/rquickjs) dengan
  [QuickJS-ng](https://github.com/quickjs-ng/quickjs),
  [axum](https://github.com/tokio-rs/axum),
  [clap](https://github.com/clap-rs/clap),
  [Ratatui](https://ratatui.rs), [Tauri](https://tauri.app) dan
  [rust_xlsxwriter](https://github.com/jmcnamara/rust_xlsxwriter).
- **Web:** [Svelte](https://svelte.dev), [Vite](https://vite.dev),
  [CodeMirror](https://codemirror.net) dan
  [GraphQL.js](https://github.com/graphql/graphql-js).
- **Desain:** font [Google Sans Flex](https://github.com/googlefonts/googlesans-flex)
  dan [Google Sans Code](https://github.com/googlefonts/googlesans-code),
  [Material Symbols](https://github.com/google/material-design-icons),
  color role dari [Material Color Utilities](https://github.com/material-foundation/material-color-utilities),
  dan Orbiting Circles dari [Magic UI](https://github.com/magicuidesign/magicui).
- **Contoh:** [JSONPlaceholder](https://jsonplaceholder.typicode.com),
  API uji coba yang dipanggil `getman init --example`.

Lisensi dan pemberitahuan untuk semua yang dibundel ada di [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Setiap dependency beserta versinya ada di `Cargo.lock` dan `web/package-lock.json`. Terima kasih kepada [semua yang sudah berkontribusi](https://github.com/fzn0x/getman/graphs/contributors) lewat kode, issue, dan review. Perubahan di tiap rilis ada di [CHANGELOG.md](CHANGELOG.md), dan semua yang ikut serta mengikuti [Code of Conduct](CODE_OF_CONDUCT.md).

## Lisensi

Dilisensikan di bawah [GNU General Public License versi 3](LICENSE-GPL) atau versi yang lebih baru, atau [lisensi MIT](LICENSE-MIT), sesuai pilihan kamu. Kontribusi diterima dengan syarat yang sama. Rilis sampai 0.3.4 berlisensi MIT atau Apache-2.0 dan tetap begitu. Font di `web/public/fonts` berada di bawah SIL Open Font License dan ikonnya adalah Material Symbols di bawah Apache License, lihat [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Postman adalah trademark dari Postman, Inc. Getman tidak berafiliasi dengan atau didukung oleh Postman.
