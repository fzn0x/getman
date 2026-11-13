# Memulai


## Install

Getman adalah satu program tanpa runtime yang perlu di-install. Pilih salah satu:

**Dengan install script** (Linux dan macOS):

```sh
curl -fsSL https://fzn0x.github.io/getman/install.sh | sh
```

**Dengan install script** (Windows, PowerShell):

```powershell
irm https://fzn0x.github.io/getman/install.ps1 | iex
```

Kedua script mengunduh rilis terbaru untuk sistem kamu dari GitHub, memeriksa checksum SHA-256-nya terhadap `SHA256SUMS` milik rilis tersebut, dan meng-install `getman` hanya untuk user kamu. Di Linux dan macOS, programnya masuk ke `~/.local/bin`, dan script akan memberi tahu apa yang perlu ditambahkan ke `PATH` kamu kalau direktori itu belum ada di sana. Di Windows, programnya masuk ke `%LOCALAPPDATA%\Programs\getman`, yang ditambahkan script ke user `PATH` kamu.

Atur `GETMAN_VERSION` (misalnya `v0.1.0`) untuk meng-install rilis tertentu, atau `GETMAN_INSTALL_DIR` untuk memilih direktorinya.

**Secara manual:** unduh archive untuk sistem kamu dari [halaman rilis](https://github.com/fzn0x/getman/releases), ekstrak, dan taruh `getman` (atau `getman.exe`) di tempat yang ada di `PATH` kamu.

**Dari source:** kamu butuh [Rust](https://rustup.rs) 1.88 atau lebih baru dan [Node.js](https://nodejs.org) 22.12 atau lebih baru.

```sh
git clone https://github.com/fzn0x/getman.git
cd getman/web
npm ci
npm run build
cd ..
cargo install --path crates/getman
```

UI browser di-build dulu karena tertanam di dalam binary.

Periksa apakah sudah jalan:

```sh
getman --version
```

## Request pertama kamu

Buat workspace dengan contoh request, di dalam folder baru:

```sh
mkdir my-api && cd my-api
getman init --example
```

`getman init` menulis `getman.yaml`, yang menandai folder itu sebagai workspace, dan sebuah `.gitignore` yang menjaga file khusus mesin tetap keluar dari git. `--example` menambahkan beberapa request ke [JSONPlaceholder](https://jsonplaceholder.typicode.com), sebuah API tes gratis, dan dua environment.

Sekarang buka:

```sh
getman
```

Getman menjalankan server di `http://127.0.0.1:7878` (atau port bebas berikutnya) dan membuka browser kamu ke sana. Tekan <kbd>Ctrl</kbd> <kbd>K</kbd> (atau <kbd>Cmd</kbd> <kbd>K</kbd> di Mac), ketik `list`, tekan Enter untuk membuka **List posts**, lalu <kbd>Ctrl</kbd> <kbd>Enter</kbd> untuk mengirimnya.

Kalau dijalankan di direktori yang bukan workspace, `getman` membuka workspace pribadi kamu, yang disimpan di folder data user, jadi kamu selalu punya tempat untuk mencoba request.

## Workspace di dalam repository kamu

Getman paling cocok dipakai dengan request yang berada di samping kode yang dipanggilnya. Di dalam repository kamu:

```sh
mkdir api && cd api
getman init --name "Orders API"
```

Lalu tambahkan request di UI, atau dengan tangan: masing-masing adalah file YAML.

```yaml
# api/orders/create-order.yaml
name: Create order
method: POST
url: "{{baseUrl}}/orders"
headers:
  - name: Accept
    value: application/json
body:
  type: json
  content: |
    { "sku": "BOOK-42", "quantity": 1 }
checks:
  - status == 201
  - json.id exists
captures:
  orderId: json.id
```

Commit file YAML-nya. Semua yang hanya milik kamu, seperti token dan override pribadi, masuk ke file `*.local.yaml`, yang diabaikan oleh `.gitignore` yang dibuat otomatis. Lihat [Workspaces](workspaces.md) untuk format lengkapnya.

## Tiga cara masuk

| Perintah | Apa yang dibuka |
| --- | --- |
| `getman` | UI browser, di 127.0.0.1 |
| `getman tui` | UI terminal, untuk sesi SSH dan penggemar keyboard |
| `getman run` | Semua request, dengan hasil untuk orang atau untuk CI |

Ketiganya membaca dan menulis file yang sama, jadi kamu bisa mengedit request di browser, mengirimnya dari terminal, dan menjalankannya di CI tanpa mengekspor apa pun.

## Keyboard

| Tombol | Fungsinya |
| --- | --- |
| <kbd>Ctrl</kbd> <kbd>Enter</kbd> | Kirim request |
| <kbd>Esc</kbd> | Batalkan request yang sedang dikirim |
| <kbd>Ctrl</kbd> <kbd>S</kbd> | Simpan |
| <kbd>Ctrl</kbd> <kbd>K</kbd> | Cari request atau perintah |
| <kbd>Alt</kbd> <kbd>N</kbd> | Request baru |
| <kbd>Alt</kbd> <kbd>L</kbd> | Edit URL |
| <kbd>Alt</kbd> <kbd>E</kbd> | Ganti environment |
| <kbd>Alt</kbd> <kbd>C</kbd> | Salin request sebagai kode |
| <kbd>Alt</kbd> <kbd>W</kbd> | Tutup tab |
| <kbd>Alt</kbd> <kbd>Shift</kbd> <kbd>T</kbd> | Buka lagi tab yang terakhir ditutup |
| <kbd>Alt</kbd> <kbd>[</kbd> <kbd>Alt</kbd> <kbd>]</kbd> | Tab sebelumnya dan berikutnya |
| <kbd>/</kbd> | Filter sidebar |
| <kbd>?</kbd> | Tampilkan semua shortcut |

Di Mac, <kbd>Cmd</kbd> menggantikan <kbd>Ctrl</kbd>. Tombol tab dan jendela memakai <kbd>Alt</kbd> karena browser menyimpan <kbd>Ctrl</kbd> <kbd>N</kbd>, <kbd>Ctrl</kbd> <kbd>W</kbd>, dan <kbd>Ctrl</kbd> <kbd>Tab</kbd> untuk dirinya sendiri.

## Berikutnya

- [Workspaces](workspaces.md): file-file dan bagaimana mereka saling terkait
- [UI browser](browser-ui.md): tab, history, trash, dan lainnya
- [Variable dan environment](variables.md)
- [Check dan script](checks.md)
- [Menjalankan di CI](runner.md)
- [Datang dari Postman](why-getman.md#moving-over)
