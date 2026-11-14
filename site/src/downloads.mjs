// Every download of the landing page, in one place. Each platform lists its
// main download, its other files and its package-manager commands.
//
// PROVISIONAL: the entries marked `soon: true` ship with the release after
// 0.3.4 and their exact file names and commands are still to be confirmed.
// Change them here and nowhere else on the landing page. The build reads the
// latest release's file names from the GitHub API: a file the release lacks
// is shown as "Coming with the next release" instead of a link, and the
// page needs no edit when the release arrives.

const dl = (file) => ({ file });
const soon = (file) => ({ file, soon: true });

/** The new file whose presence tells that the release has the new commands. */
export const marker = "Getman-android-universal.apk";

export const platforms = [
  {
    id: "windows",
    name: "Windows",
    requires: "Windows 10 or newer, x64 or ARM64",
    main: [
      { label: "Download for Windows x64", ...dl("Getman-windows-x64-setup.exe") },
      { label: "ARM64", ...dl("Getman-windows-arm64-setup.exe"), tonal: true },
    ],
    note: "The setup installs for your user and adds getman to your PATH.",
    files: [
      { label: "x64 MSI", ...dl("Getman-windows-x64.msi") },
      { label: "x64 portable zip", ...dl("Getman-windows-x64-portable.zip") },
      { label: "ARM64 MSI", ...dl("Getman-windows-arm64.msi") },
      { label: "ARM64 portable zip", ...dl("Getman-windows-arm64-portable.zip") },
    ],
    commands: [
      { label: "CLI", text: "irm https://fzn0x.github.io/getman/install.ps1 | iex" },
      {
        label: "Scoop",
        text: "scoop bucket add fzn0x https://github.com/fzn0x/scoop-bucket; scoop install fzn0x/getman",
      },
    ],
  },
  {
    id: "macos",
    name: "macOS",
    requires: "macOS 10.13 or newer, Apple silicon and Intel",
    main: [{ label: "Download for macOS", ...dl("Getman-macos-universal.dmg") }],
    note: "Open the disk image and drag Getman to Applications.",
    files: [],
    commands: [
      { label: "Cask", text: "brew install --cask fzn0x/tap/getman-desktop", soon: true },
      { label: "CLI", text: "brew install fzn0x/tap/getman" },
      { label: "CLI", text: "curl -fsSL https://fzn0x.github.io/getman/install.sh | sh" },
    ],
  },
  {
    id: "linux",
    name: "Linux",
    requires: "glibc 2.28 or newer, x86_64 or aarch64",
    main: [
      { label: "Download for Linux x86_64", ...dl("Getman-linux-x86_64.AppImage") },
      { label: "aarch64", ...dl("Getman-linux-aarch64.AppImage"), tonal: true },
    ],
    note: "Make the AppImage executable with chmod +x and run it.",
    files: [
      { label: "x86_64 .deb", ...dl("getman-desktop_amd64.deb") },
      { label: "x86_64 .rpm", ...dl("getman-desktop.x86_64.rpm") },
      { label: "x86_64 Flatpak", ...soon("Getman-linux-x86_64.flatpak") },
      { label: "aarch64 .deb", ...dl("getman-desktop_arm64.deb") },
      { label: "aarch64 .rpm", ...dl("getman-desktop.aarch64.rpm") },
      { label: "aarch64 Flatpak", ...soon("Getman-linux-aarch64.flatpak") },
    ],
    commands: [
      {
        label: "Flatpak",
        text: "flatpak install --user Getman-linux-x86_64.flatpak",
        soon: true,
      },
      { label: "AUR", text: "yay -S getman-bin", soon: true },
      { label: "Nix", text: "nix run github:fzn0x/getman", soon: true },
      { label: "CLI", text: "curl -fsSL https://fzn0x.github.io/getman/install.sh | sh" },
    ],
  },
  {
    id: "android",
    name: "Android",
    requires: "Android phones and tablets, one APK for every device",
    main: [{ label: "Download for Android", ...soon("Getman-android-universal.apk") }],
    note: "The APK is signed and installs directly, without Google Play.",
    // PROVISIONAL steps, to confirm.
    steps: [
      "Download the APK on the phone.",
      "Open it, and allow installs from this source when Android asks.",
      "Tap Install.",
    ],
    files: [],
    commands: [{ label: "adb", text: "adb install Getman-android-universal.apk", soon: true }],
  },
  {
    id: "ios",
    name: "iOS",
    requires: "iPhone and iPad, installed by sideloading",
    main: [{ label: "Download for iOS", ...soon("Getman-ios.ipa") }],
    note: "The IPA is unsigned and there is no App Store version. A free Apple ID must refresh the app every 7 days.",
    // PROVISIONAL steps, to confirm.
    steps: [
      "Download the IPA.",
      "Open it in AltStore, SideStore or Sideloadly, which sign it with your Apple ID.",
      "Refresh the app from the same tool every 7 days.",
    ],
    files: [],
    commands: [],
  },
];

/**
 * Whether a file can be linked. `released` is the set of the latest
 * release's file names, or null when the API could not be read: then only
 * the files that exist since 0.3.4 count.
 */
export function available(entry, released) {
  if (released) return released.has(entry.file);
  return !entry.soon;
}

/** Whether a command that ships with the new release works yet. */
export function commandReady(command, released) {
  if (!command.soon) return true;
  return released ? released.has(marker) : false;
}
