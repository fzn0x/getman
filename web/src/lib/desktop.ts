// What the desktop app tells the UI, and how the UI words it. The UI runs
// in a browser tab too, where none of this exists: everything here is
// shown only when `inDesktopApp()` says so.

/** How this copy of the app was installed. */
export type Install =
  "development" | "nsis" | "msi" | "portable" | "app" | "appimage" | "deb" | "rpm" | "other";

/** Where an update stands. */
export type UpdatePhase =
  | { state: "idle" }
  | { state: "checking"; manual: boolean }
  | { state: "upToDate" }
  | { state: "available"; version: string; notes: string; date: string | null }
  | { state: "downloading"; version: string; done: number; total: number | null }
  | { state: "installed"; version: string }
  | { state: "failed"; message: string };

export interface UpdateStatus {
  version: string;
  install: Install;
  /** Why this copy doesn't update itself, or null when it does. */
  notUpdating: string | null;
  checkForUpdates: boolean;
  installWithoutAsking: boolean;
  /** When the app last looked, in seconds since 1970. */
  lastCheck: number | null;
  /** Whether a new version installs as Getman restarts, as on Windows. */
  installsOnRestart: boolean;
  phase: UpdatePhase;
}

export interface CommandLine {
  /** Whether this copy offers to install the getman command (macOS). */
  offered: boolean;
  /** Where the command links to this copy, if it does. */
  installed: string | null;
}

export interface DesktopState {
  updates: UpdateStatus;
  commandLine: CommandLine;
}

/** Whether the UI runs inside the desktop app rather than a browser tab. */
export function inDesktopApp(scope: object = globalThis): boolean {
  return "__TAURI_INTERNALS__" in scope;
}

const INSTALLS: Record<Install, string> = {
  development: "Development build",
  nsis: "Installed with the setup program",
  msi: "Installed with the Windows Installer package",
  portable: "Portable copy",
  app: "macOS app",
  appimage: "AppImage",
  deb: "Installed from the .deb package",
  rpm: "Installed from the .rpm package",
  other: "Release build",
};

/** How a copy was installed, in words. */
export function installLabel(install: Install): string {
  return INSTALLS[install] ?? install;
}

/** When the app last looked for a new version, in words. */
export function lastChecked(seconds: number | null, now: Date = new Date()): string {
  if (seconds === null) return "Not checked yet";
  const ago = Math.max(0, Math.round(now.getTime() / 1000 - seconds));
  if (ago < 60) return "Checked just now";
  if (ago < 60 * 60) {
    const minutes = Math.floor(ago / 60);
    return `Checked ${minutes} minute${minutes === 1 ? "" : "s"} ago`;
  }
  if (ago < 24 * 60 * 60) {
    const hours = Math.floor(ago / 3600);
    return `Checked ${hours} hour${hours === 1 ? "" : "s"} ago`;
  }
  const date = new Date(seconds * 1000);
  return `Last checked ${date.toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    year: "numeric",
  })}, ${date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}`;
}

/** A download's progress in whole percent, or null while its size is unknown. */
export function percent(done: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.min(100, Math.floor((done / total) * 100));
}

/** A release's date, such as "28 Oct 2026", or "" when it has none. */
export function releaseDate(date: string | null): string {
  if (!date) return "";
  const parsed = new Date(date);
  if (Number.isNaN(parsed.getTime())) return "";
  return parsed.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}

/**
 * Release notes as plain text: GitHub's Markdown without heading marks,
 * emphasis or link targets, and without the full changelog link.
 */
export function plainNotes(notes: string): string {
  return notes
    .replace(/\r\n/g, "\n")
    .split("\n")
    .filter((line) => !/^\*\*Full Changelog\*\*/.test(line.trim()))
    .map((line) =>
      line
        .replace(/^#{1,6}\s+/, "")
        .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
        .replace(/\*\*([^*]+)\*\*/g, "$1")
        .replace(/^\s*[*-]\s+/, "• "),
    )
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}
