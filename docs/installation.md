# Installation

Getman comes as a desktop app, which brings the `getman` command with it,
and as a command line on its own. This page covers every way to install,
verify, update and remove both.

The two work on the same files, so you can switch between them, or use
both, at any time.

| You want | Install |
| --- | --- |
| Getman in a window of its own, plus the `getman` command | The [desktop app](#desktop-app) |
| Only the `getman` command, which opens the UI in your browser | The [command line](#command-line) |
| Getman on a server, in CI or in a container | The command line, with the [install script](#install-script-for-linux-and-macos) |

[Getting started](getting-started.md) has the short version of this page.
Every release, with its notes and files, is on the
[releases page](https://github.com/fzn0x/getman/releases).

## Desktop app

The desktop files keep the same names in every release, so these links
always fetch the newest version:

| System | Download |
| --- | --- |
| Windows x64 | [Setup](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-x64-setup.exe) · [MSI](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-x64.msi) · [Portable zip](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-x64-portable.zip) |
| Windows ARM64 | [Setup](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-arm64-setup.exe) · [MSI](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-arm64.msi) · [Portable zip](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-arm64-portable.zip) |
| macOS (Apple silicon and Intel) | [Disk image](https://github.com/fzn0x/getman/releases/latest/download/Getman-macos-universal.dmg) |
| Linux x86_64 | [AppImage](https://github.com/fzn0x/getman/releases/latest/download/Getman-linux-x86_64.AppImage) · [.deb](https://github.com/fzn0x/getman/releases/latest/download/getman-desktop_amd64.deb) · [.rpm](https://github.com/fzn0x/getman/releases/latest/download/getman-desktop.x86_64.rpm) |
| Linux ARM64 | [AppImage](https://github.com/fzn0x/getman/releases/latest/download/Getman-linux-aarch64.AppImage) · [.deb](https://github.com/fzn0x/getman/releases/latest/download/getman-desktop_arm64.deb) · [.rpm](https://github.com/fzn0x/getman/releases/latest/download/getman-desktop.aarch64.rpm) |

The app is not signed with a paid certificate, so Windows and macOS warn
the first time you open it. [Troubleshooting](troubleshooting.md#desktop-app)
shows how to get past each warning once.

### Windows

There are three Windows downloads. Most people want the setup.

| | Setup | MSI | Portable zip |
| --- | --- | --- | --- |
| File | `Getman-windows-x64-setup.exe` | `Getman-windows-x64.msi` | `Getman-windows-x64-portable.zip` |
| Installs for | You only | Every user of the computer | Nobody: it runs where you unpack it |
| Needs an administrator | No | Yes | No |
| Installs in | `%LOCALAPPDATA%\Getman` | `C:\Program Files\Getman` | The folder you choose |
| The `getman` command | `bin\getman.exe`, added to your user `PATH` | `bin\getman.exe`, added to the system `PATH` | `bin\getman.exe`, not on `PATH` |
| WebView2 Runtime | Installed if missing | Installed if missing | Must be there already |
| Updates itself | Yes | Yes | No |

For ARM64 Windows, take the files with `arm64` in place of `x64`. They
bring a native ARM64 `getman.exe` too. Getman needs Windows 10 or newer.

**Setup.** Run it. The welcome page has a ticked box, **Add the getman
command to PATH**; untick it to leave your `PATH` alone. Open a new
terminal afterwards to use `getman`.

**MSI.** Run it, or install it from a terminal. `ADDTOPATH=0` leaves the
system `PATH` alone:

```powershell
msiexec /i Getman-windows-x64.msi
msiexec /i Getman-windows-x64.msi ADDTOPATH=0
```

**Portable zip.** Unpack it anywhere, such as a USB stick, and run
`Getman\Getman.exe`. The command line is `Getman\bin\getman.exe`. The zip
needs the Microsoft Edge WebView2 Runtime, which Windows 10 and 11
normally have; the setup and the MSI install it when it is missing.

To install without questions, for example from a deployment script, the
setup takes `/S` and the MSI takes `/qn`. A quiet setup still adds
`getman` to `PATH` on a first install:

```powershell
.\Getman-windows-x64-setup.exe /S
msiexec /i Getman-windows-x64.msi /qn ADDTOPATH=0
```

### macOS

Open `Getman-macos-universal.dmg` and drag **Getman** to
**Applications**. It is one app for Apple silicon and Intel Macs, and
needs macOS 10.13 or newer. The first time you open it, macOS asks you
to allow it; see [macOS cannot check Getman for malicious
software](troubleshooting.md#macos-cannot-check-getman-for-malicious-software).

To use `getman` in a terminal, open **Settings › About** in the app and
choose **Install the getman command**. It creates a link named `getman`
to the app's own program, in `/usr/local/bin`, or in `~/.local/bin` when
it cannot write to `/usr/local/bin`. The link follows the app, so the
command updates with it. The button replaces a `getman` link only when it
points into a `Getman.app`. A file, or a link to anything else, such as
Homebrew's, is left as it is, so one installed another way keeps working.

The button is not offered while the app runs from the mounted disk
image: drag it to **Applications** first.

### Linux

**AppImage.** It runs on most distributions without installing:

```sh
chmod +x Getman-linux-x86_64.AppImage
./Getman-linux-x86_64.AppImage
```

Some distributions need FUSE 2 for AppImages: `sudo apt install libfuse2`
on Ubuntu 22.04, `libfuse2t64` on 24.04. Given a command, the AppImage is
the command line instead of the window:

```sh
./Getman-linux-x86_64.AppImage run --bail
```

**.deb** for Debian, Ubuntu and their relatives, and **.rpm** for Fedora,
RHEL and openSUSE:

```sh
sudo apt install ./getman-desktop_amd64.deb
sudo dnf install ./getman-desktop.x86_64.rpm
```

The packages add Getman to your applications and put the `getman` command
in `/usr/bin`; `getman desktop` opens the window. They need WebKitGTK 4.1,
which the package manager installs with them. The installed package is
named `getman`.

For ARM64, take the files with `aarch64` or `arm64` in their names. The
desktop files are built on Ubuntu 22.04, so they run on distributions with
the same glibc or a newer one.

## Command line

These routes install only the `getman` command. It has the browser UI
built in: `getman` opens it in your browser, and
[Command line](cli.md) lists everything else it does.

### Install script for Linux and macOS

```sh
curl -fsSL https://fzn0x.github.io/getman/install.sh | sh
```

The script:

1. Picks the build for your system: Linux or macOS, x86_64 or ARM64. A
   shell running under Rosetta on an Apple silicon Mac still gets the
   native build.
2. Finds the newest release by following
   `https://github.com/fzn0x/getman/releases/latest`, unless you name a
   version.
3. Downloads `getman-<version>-<target>.tar.gz` and the release's
   `SHA256SUMS` over HTTPS, and checks the archive's SHA-256. If it does
   not match, nothing is installed.
4. Puts `getman` in `~/.local/bin`, creating the directory if needed. An
   existing `getman` is replaced in one step, so a running one is not
   disturbed.
5. Says what to add to your `PATH` if the directory is not on it yet. It
   never edits your shell's startup files itself.

It needs `curl` or `wget`, `tar`, and `sha256sum` or `shasum`. Two
environment variables change what it does:

| Variable | Default | Does |
| --- | --- | --- |
| `GETMAN_VERSION` | `latest` | The release to install, by its tag with the `v`, such as `v0.3.4` |
| `GETMAN_INSTALL_DIR` | `~/.local/bin` | The directory to put `getman` in |

Set them for `sh`, after the pipe:

```sh
curl -fsSL https://fzn0x.github.io/getman/install.sh | GETMAN_VERSION=v0.3.4 sh
curl -fsSL https://fzn0x.github.io/getman/install.sh | GETMAN_INSTALL_DIR="$HOME/bin" sh
curl -fsSL https://fzn0x.github.io/getman/install.sh | sudo GETMAN_INSTALL_DIR=/usr/local/bin sh
```

When `~/.local/bin` is not on your `PATH`, add this line to your shell's
startup file, such as `~/.profile`, `~/.bashrc` or `~/.zshrc`, and open a
new terminal:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

To read the script before you run it:

```sh
curl -fsSLO https://fzn0x.github.io/getman/install.sh
sh install.sh
```

The Linux build needs glibc; it does not run on musl systems such as
Alpine. Releases after 0.3.4 need glibc 2.28 or newer, and 0.3.4 needs
2.39. See [GLIBC not found on Linux](troubleshooting.md#glibc-not-found-on-linux)
for which distributions it runs on. In CI, add the directory to the
job's `PATH`, as in [Running in CI](runner.md#github-actions).

### Install script for Windows

In PowerShell:

```powershell
irm https://fzn0x.github.io/getman/install.ps1 | iex
```

The script:

1. Asks the GitHub API for the newest release, unless you name a version.
2. Downloads the release's `SHA256SUMS`, then the zip for your system:
   `getman-<version>-x86_64-pc-windows-msvc.zip`, or
   `getman-<version>-aarch64-pc-windows-msvc.zip` on ARM64 Windows. It
   checks the zip's SHA-256. If it does not match, nothing is installed.
3. Puts `getman.exe` in `%LOCALAPPDATA%\Programs\getman`. A running
   `getman.exe` is renamed out of the way first, since Windows cannot
   overwrite a running program.
4. Adds that folder to your user `PATH` if it is not there yet, and to the
   `PATH` of the terminal it runs in. Other terminals that are open
   already need a restart.

On ARM64 Windows, it installs the native ARM64 build when the release
has one. Releases up to 0.3.4 have no ARM64 command-line archive, so for
those the script says so and installs the x64 build, which Windows 11 runs
through emulation. The [desktop app](#windows) has a native ARM64
build. There is no build for 32-bit Windows.

The same two variables work, set in the terminal before the script runs:

| Variable | Default | Does |
| --- | --- | --- |
| `GETMAN_VERSION` | `latest` | The release to install, by its tag with the `v`, such as `v0.3.4` |
| `GETMAN_INSTALL_DIR` | `%LOCALAPPDATA%\Programs\getman` | The folder to put `getman.exe` in, which is added to your user `PATH` |

```powershell
$env:GETMAN_VERSION = 'v0.3.4'
irm https://fzn0x.github.io/getman/install.ps1 | iex
```

The variable stays set in that terminal, so a second run installs the
same version again. `Remove-Item Env:GETMAN_VERSION` goes back to the
newest release.

### Homebrew

On macOS and Linux:

```sh
brew install fzn0x/tap/getman
```

The formula `getman` lives in the tap
[fzn0x/homebrew-tap](https://github.com/fzn0x/homebrew-tap), which each
release updates. It installs the command line for Intel and ARM, from
the same archives as the install script, and checks their SHA-256.

### Scoop

On Windows:

```powershell
scoop bucket add fzn0x https://github.com/fzn0x/scoop-bucket
scoop install fzn0x/getman
```

The manifest `getman` lives in the bucket
[fzn0x/scoop-bucket](https://github.com/fzn0x/scoop-bucket), which each
release updates. It installs the x64 command line, or the ARM64 one on
ARM64 Windows, and puts `getman` on your `PATH` through Scoop's shims.
The ARM64 entry works from the first release that has an ARM64 archive;
0.3.4 and older have none.

### Archives

To install by hand, download the archive for your system from the
[releases page](https://github.com/fzn0x/getman/releases). Its name holds
the version:

| System | Archive |
| --- | --- |
| Linux x86_64 | `getman-v0.3.4-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `getman-v0.3.4-aarch64-unknown-linux-gnu.tar.gz` |
| macOS, Apple silicon | `getman-v0.3.4-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `getman-v0.3.4-x86_64-apple-darwin.tar.gz` |
| Windows x64, and ARM64 through emulation | `getman-v0.3.4-x86_64-pc-windows-msvc.zip` |
| Windows ARM64, native | `getman-<version>-aarch64-pc-windows-msvc.zip`, from the release after 0.3.4 on |

Each holds a folder of the same name with `getman` (`getman.exe` on
Windows), `README.md`, the licence files and `THIRD_PARTY_NOTICES.md`.
Put the program in a directory on your `PATH`.
On Linux and macOS:

```sh
version=v0.3.4
target=x86_64-unknown-linux-gnu
curl -fsSLO "https://github.com/fzn0x/getman/releases/download/$version/getman-$version-$target.tar.gz"
tar -xzf "getman-$version-$target.tar.gz"
mkdir -p ~/.local/bin
install -m 755 "getman-$version-$target/getman" ~/.local/bin/getman
```

On Windows, unpack the zip with `Expand-Archive` or Explorer, and move
`getman.exe` to a folder on your `PATH`. Then check that it works:

```sh
getman --version
```

## Verifying a download

Every release has a `SHA256SUMS` file with the SHA-256 of each of its
files. The install scripts check it for you, and Homebrew and Scoop check
the hashes in their formula and manifest. For a file you downloaded
yourself, download `SHA256SUMS` from the same release and compare.

On Linux, in the folder with the download:

```sh
curl -fsSLO https://github.com/fzn0x/getman/releases/latest/download/SHA256SUMS
grep ' Getman-linux-x86_64.AppImage$' SHA256SUMS | sha256sum -c
```

On macOS:

```sh
curl -fsSLO https://github.com/fzn0x/getman/releases/latest/download/SHA256SUMS
grep ' Getman-macos-universal.dmg$' SHA256SUMS | shasum -a 256 -c
```

Both print the file's name and `OK` when it matches. With GNU
`sha256sum`, `sha256sum --check --ignore-missing SHA256SUMS` checks every
download in the folder at once.

On Windows, in PowerShell:

```powershell
Invoke-WebRequest https://github.com/fzn0x/getman/releases/latest/download/SHA256SUMS -OutFile SHA256SUMS
$file = 'Getman-windows-x64-setup.exe'
$line = Get-Content SHA256SUMS | Where-Object { $_.EndsWith(" $file") }
$actual = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
if ($line.Split(' ')[0] -eq $actual) { 'OK' } else { 'MISMATCH' }
```

`releases/latest/download/SHA256SUMS` belongs to the newest release. For
a file from an older one, take that release's, such as
`https://github.com/fzn0x/getman/releases/download/v0.3.4/SHA256SUMS`.

The `.sig` files and `latest.json` in a release are for the desktop
app's updater, which checks each update's signature itself. You need
nothing of them to install by hand.

## Updating

| Installed with | To update |
| --- | --- |
| Windows setup or MSI | The app updates itself. Or run the newer setup or MSI over it |
| macOS disk image | The app updates itself. Or drag the newer app over it in Applications |
| AppImage | The app updates itself. Or replace the file with the newer one |
| .deb | `sudo apt install ./getman-desktop_amd64.deb` with the newer file |
| .rpm | `sudo dnf install ./getman-desktop.x86_64.rpm` with the newer file |
| Portable zip | Download the new zip and unpack it in place of the old one |
| Install script | Run the script again |
| Homebrew | `brew upgrade getman` |
| Scoop | `scoop update getman` |
| An archive by hand | Replace the program with the newer one |

The desktop app looks for a new release once a day, shows what changed
and installs it when you choose **Install**. You can let it install
without asking, or turn the check off, in **Settings › About**. Every
update is signed, and the app refuses one whose signature does not match.
[Updates](desktop.md#updates) has the details. The `getman` command that
came with the app updates with it.

The command line never updates itself and never looks for new versions.

## Uninstalling

| Installed with | To remove |
| --- | --- |
| Windows setup | **Settings › Apps › Installed apps**, then Getman, then **Uninstall**. It takes `bin` off your `PATH`. Tick **Delete the application data** to remove the app's settings too |
| Windows MSI | The same, or `msiexec /x Getman-windows-x64.msi`. It takes `bin` off the system `PATH` |
| Portable zip | Delete the folder |
| macOS disk image | Quit Getman and drag it from Applications to the Trash. If you installed the `getman` command, delete the link: `rm /usr/local/bin/getman` or `rm ~/.local/bin/getman` |
| AppImage | Delete the file |
| .deb | `sudo apt remove getman` |
| .rpm | `sudo dnf remove getman` |
| Install script, Linux and macOS | `rm ~/.local/bin/getman`, and remove the `PATH` line if you added one |
| Install script, Windows | Delete `%LOCALAPPDATA%\Programs\getman`, then remove it from **Path** in **Edit environment variables for your account** |
| Homebrew | `brew uninstall getman`, then `brew untap fzn0x/tap` if you use nothing else from it |
| Scoop | `scoop uninstall getman`, then `scoop bucket rm fzn0x` if you use nothing else from it |

If you chose another directory with `GETMAN_INSTALL_DIR`, remove `getman`
from there instead.

### What stays behind

Uninstalling never touches your workspaces. Getman also keeps a few
files in your user folders, which every kind of install shares:

| | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Personal workspace | `%APPDATA%\getman\workspace` | `~/Library/Application Support/getman/workspace` | `~/.local/share/getman/workspace` |
| The UI's token | `%APPDATA%\getman\ui-token` | `~/Library/Application Support/getman/ui-token` | `~/.config/getman/ui-token` |
| Desktop app settings | `%APPDATA%\io.github.fzn0x.getman` | `~/Library/Application Support/io.github.fzn0x.getman` | `~/.config/io.github.fzn0x.getman` |
| Desktop app UI storage | `%LOCALAPPDATA%\io.github.fzn0x.getman` | `~/Library/WebKit/io.github.fzn0x.getman` | `~/.local/share/io.github.fzn0x.getman` |

Delete them to remove every trace. The personal workspace holds the
requests you made there, so keep it if you want them. The Windows
setup's **Delete the application data** box removes only the desktop
app's two folders. [Where the app keeps its
settings](desktop.md#where-the-app-keeps-its-settings) says what is in
them.

## Building from source

When no release fits your system, build Getman yourself. You need
[Rust](https://rustup.rs) 1.88 or newer and
[Node.js](https://nodejs.org) 22.12 or newer. The browser UI is built
first, because it is embedded in the binary:

```sh
git clone https://github.com/fzn0x/getman.git
cd getman/web
npm ci
npm run build
cd ..
cargo install --path crates/getman
```

The desktop app needs Rust 1.90 or newer and, on Linux, the WebKitGTK
development libraries. [Development](development.md) covers both builds,
the tests and making the installers.
