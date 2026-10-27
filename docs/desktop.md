# Desktop app

The desktop app is Getman's UI in a window of its own, for Windows, macOS
and Linux. It works on the same files as the `getman` command, with the
same server inside it on `127.0.0.1`, so nothing else needs to run. It
opens the workspace you used last, keeps itself up to date if you let it,
and brings the `getman` command along.

## Install

Every release has these downloads, under the same names each time:

| System | Download |
| --- | --- |
| Windows x64 | [Setup](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-x64-setup.exe) · [MSI](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-x64.msi) · [Portable zip](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-x64-portable.zip) |
| Windows ARM64 | [Setup](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-arm64-setup.exe) · [MSI](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-arm64.msi) · [Portable zip](https://github.com/fzn0x/getman/releases/latest/download/Getman-windows-arm64-portable.zip) |
| macOS (Apple silicon and Intel) | [Disk image](https://github.com/fzn0x/getman/releases/latest/download/Getman-macos-universal.dmg) |
| Linux x86_64 | [AppImage](https://github.com/fzn0x/getman/releases/latest/download/Getman-linux-x86_64.AppImage) · [.deb](https://github.com/fzn0x/getman/releases/latest/download/getman-desktop_amd64.deb) · [.rpm](https://github.com/fzn0x/getman/releases/latest/download/getman-desktop.x86_64.rpm) |
| Linux ARM64 | [AppImage](https://github.com/fzn0x/getman/releases/latest/download/Getman-linux-aarch64.AppImage) · [.deb](https://github.com/fzn0x/getman/releases/latest/download/getman-desktop_arm64.deb) · [.rpm](https://github.com/fzn0x/getman/releases/latest/download/getman-desktop.aarch64.rpm) |

The release's `SHA256SUMS` lists the checksum of each file. The app is
not signed with a paid certificate, so Windows and macOS warn the first
time you open it; the steps below get past that once.

### Windows

**Setup** (recommended): run `Getman-windows-x64-setup.exe`. It installs
Getman for your user only, with no administrator rights, in
`%LOCALAPPDATA%\Getman`. If Windows says "Windows protected your PC",
choose **More info**, then **Run anyway**.

The welcome page has a box, ticked, to add the `getman` command to your
`PATH`. Open a new terminal afterwards to use it.

**MSI**: `Getman-windows-x64.msi` installs for every user of the
computer, in `Program Files`, and needs an administrator. It adds the
`getman` command to the system `PATH`; to leave `PATH` alone, install it
from a terminal with `msiexec /i Getman-windows-x64.msi ADDTOPATH=0`.

**Portable zip**: unpack `Getman-windows-x64-portable.zip` anywhere, such
as a USB stick, and run `Getman.exe`. The command line is
`bin\getman.exe`. It needs the Microsoft Edge WebView2 Runtime, which
Windows 10 and 11 already have. The portable copy never replaces itself:
to update it, download the new zip.

Getman needs Windows 10 or newer. The ARM64 downloads are for Windows on
ARM computers.

### macOS

Open `Getman-macos-universal.dmg` and drag **Getman** to
**Applications**. It runs natively on Apple silicon and Intel Macs, on
macOS 10.13 or newer.

The first time you open it, macOS says it cannot check Getman for
malicious software, because the app is not notarized by Apple. Click
**Done**, open **System Settings › Privacy & Security**, scroll to the
message about Getman and click **Open Anyway**, then confirm. After that
it opens like any app.

To use `getman` in a terminal, open **Settings › About** in the app and
choose **Install the getman command**. It links `/usr/local/bin/getman`,
or `~/.local/bin/getman` when it cannot write to `/usr/local/bin`, to the
app.

### Linux

**AppImage**: make it executable and run it.

```sh
chmod +x Getman-linux-x86_64.AppImage
./Getman-linux-x86_64.AppImage
```

Some distributions need FUSE 2 for AppImages (`sudo apt install libfuse2`
on Ubuntu 22.04, `libfuse2t64` on 24.04). Given a command, the AppImage
runs the command line instead: `./Getman-linux-x86_64.AppImage run`.

**.deb** (Debian, Ubuntu and their relatives):

```sh
sudo apt install ./getman-desktop_amd64.deb
```

**.rpm** (Fedora, RHEL, openSUSE):

```sh
sudo dnf install ./getman-desktop.x86_64.rpm
```

The packages add Getman to your applications and put the `getman`
command in `/usr/bin`. They need WebKitGTK 4.1, which the package manager
installs with them.

## One download for the window and the command line

- **macOS and Linux**: the app's program is the `getman` command itself.
  `getman desktop` opens the window, `getman desktop ./api` opens it on
  that folder, and every other command works as in
  [Command line](cli.md). Started from the Dock or the app launcher, it
  opens the window.
- **Windows**: a program is either a window or a console program, so the
  installers and the zip bring `getman.exe` in a `bin` folder beside the
  app, the same program as the command-line download.

## Workspaces

The app opens the workspace you used last, or your personal workspace
the first time. To work on another, choose **Open a workspace folder**
(the folder button at the top, or the command palette) and pick a folder:
Getman opens the workspace in it, or creates one there if it has none.
Open tabs belong to their workspace and come back when you open it again.

Links that lead out of Getman, such as one in a response, open in your
browser, not in the app's window.

## Updates

While **Check for updates automatically** is on, which it is by default,
the app looks once a day for a newer release on GitHub. When there is
one, it shows what changed, with **Install** and **Later**. Install
downloads it, checks its signature, installs it and offers to restart.
Later asks again at the next day's check.

With **Install updates without asking** on (off by default, and only
possible while checks are on), the app downloads and installs new
versions by itself, and the new version runs the next time you open
Getman. On Windows, where a running program cannot be replaced, the
installer runs as Getman restarts or quits.

Every update is signed when it is released, and the app installs it only
if the signature matches the public key built into it, so a changed or
corrupted download is refused.

Both choices, **Check now** and when Getman last looked are in
**Settings › About** (the gear at the top right, or **Settings** in the
command palette). Turn **Check for updates automatically** off to stop
the check; the app then makes no request of its own at all. **Check now**
still looks when you ask.

These copies never update themselves, and **Settings › About** says so:

- development builds;
- the .deb and .rpm packages, which your package manager updates:
  install the newer package from the releases page;
- the portable zip: download the new zip.

## Where the app keeps its settings

The app's own settings (the workspace used last, the update choices, when
it last checked and the port of its server) are in `settings.json`, and the
window's size and place in `.window-state.json`, in:

| System | Folder |
| --- | --- |
| Windows | `%APPDATA%\io.github.fzn0x.getman` |
| macOS | `~/Library/Application Support/io.github.fzn0x.getman` |
| Linux | `~/.config/io.github.fzn0x.getman` |

The UI's preferences, such as the theme, the layout and the open tabs,
live in the window's web storage: `%LOCALAPPDATA%\io.github.fzn0x.getman`
on Windows, `~/Library/WebKit/io.github.fzn0x.getman` on macOS and
`~/.local/share/io.github.fzn0x.getman` on Linux.

The personal workspace and the UI's token are the ones the `getman`
command uses, so the browser UI and the app share them. Your workspaces
stay wherever you keep them.

Uninstalling leaves these folders; delete them to start afresh. The
Windows setup's uninstaller can delete the app's data for you.

## Troubleshooting

- **The window stays blank on Linux.** Some graphics drivers trip
  WebKitGTK; start Getman with `WEBKIT_DISABLE_DMABUF_RENDERER=1` set.
- **"Getman is damaged" on macOS.** The download was quarantined while
  incomplete; download it again, or run
  `xattr -dr com.apple.quarantine /Applications/Getman.app`.
- **A second Getman does not open.** The app runs once; starting it again
  brings the open window to the front.
