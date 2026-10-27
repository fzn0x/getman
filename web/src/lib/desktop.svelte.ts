// The bridge to the desktop app: its updates, the workspace folder picker
// and the getman command. In a browser tab `startDesktop()` does nothing
// and `desktop.available` stays false, so nothing here shows.

import type { IconName } from "./icons";
import { type CommandLine, type DesktopState, inDesktopApp, type UpdateStatus } from "./desktop";
import {
  closeDialog,
  confirmDialog,
  dialogs,
  isDirty,
  message,
  notify,
  openDialog,
  tabs,
} from "./store.svelte";

export const desktop = $state({
  available: false,
  status: null as UpdateStatus | null,
  commandLine: { offered: false, installed: null } as CommandLine,
  /** The version the user put off with Later in this session. */
  later: "",
  checking: false,
});

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
let invoke: Invoke = () => Promise.reject(new Error("Getman's desktop app is not running."));

/** Connects to the desktop app, when the UI runs inside it. */
export async function startDesktop(): Promise<void> {
  if (!inDesktopApp()) return;
  const [core, event] = await Promise.all([
    import("@tauri-apps/api/core"),
    import("@tauri-apps/api/event"),
  ]);
  invoke = core.invoke;
  await event.listen<UpdateStatus>("getman://updates", (e) => (desktop.status = e.payload));
  const state = await invoke<DesktopState>("desktop_state");
  desktop.status = state.updates;
  desktop.commandLine = state.commandLine;
  desktop.available = true;
  // Offer a new version as soon as nothing else is open.
  $effect.root(() => {
    $effect(() => {
      const phase = desktop.status?.phase;
      if (phase?.state === "available" && phase.version !== desktop.later && !dialogs.current) {
        openDialog({ kind: "update" });
      }
    });
  });
}

export async function setUpdatePreferences(check: boolean, install: boolean): Promise<void> {
  try {
    desktop.status = await invoke<UpdateStatus>("set_update_preferences", { check, install });
  } catch (error) {
    notify(message(error), "error");
  }
}

/** Looks for a new version now, and offers it in the update dialog. */
export async function checkNow(): Promise<void> {
  desktop.checking = true;
  try {
    desktop.status = await invoke<UpdateStatus>("check_for_updates");
    if (desktop.status.phase.state === "available") {
      desktop.later = "";
      openDialog({ kind: "update" });
    }
  } catch (error) {
    notify(message(error), "error");
  } finally {
    desktop.checking = false;
  }
}

/** Downloads, verifies and installs the version offered. */
export async function installUpdate(): Promise<void> {
  try {
    desktop.status = await invoke<UpdateStatus>("install_update");
  } catch {
    // The status event carries the failure to the dialog.
  }
}

/** Puts the offered version off until the next check. */
export function later(): void {
  const phase = desktop.status?.phase;
  if (phase && "version" in phase) desktop.later = phase.version;
  closeDialog();
}

export function restartApp(): void {
  void invoke("restart").catch((error) => notify(message(error), "error"));
}

/** Opens another workspace folder, chosen with the system's folder picker. */
export function openWorkspaceFolder(): void {
  const unsaved = tabs.list.filter(isDirty).length;
  if (unsaved === 0) {
    void pickWorkspace();
    return;
  }
  confirmDialog({
    title: "Open another workspace?",
    body: `${unsaved} tab${unsaved === 1 ? " has" : "s have"} unsaved changes, which will be lost.`,
    confirm: "Choose a folder",
    danger: true,
    onConfirm: () => void pickWorkspace(),
  });
}

async function pickWorkspace(): Promise<void> {
  try {
    const folder = await invoke<string | null>("open_workspace");
    if (folder === null) return;
    // The tabs belong to the workspace left behind; it keeps them for
    // when it is opened again.
    tabs.list = [];
    location.reload();
  } catch (error) {
    notify(message(error), "error");
  }
}

export async function installCommand(): Promise<void> {
  try {
    desktop.commandLine = await invoke<CommandLine>("install_command");
    notify(`Installed the getman command in ${desktop.commandLine.installed}`, "ok");
  } catch (error) {
    notify(message(error), "error");
  }
}

/** The desktop app's commands for the command palette. */
export function desktopCommands(): {
  id: string;
  title: string;
  detail?: string;
  icon: IconName;
  run: () => void;
}[] {
  if (!desktop.available) return [];
  const commands = [
    {
      id: "open-workspace",
      title: "Open a workspace folder",
      detail: "Any folder: Getman creates a workspace in it if it has none",
      icon: "folder" as IconName,
      run: openWorkspaceFolder,
    },
    {
      id: "settings",
      title: "Settings",
      detail: "Updates, version and the getman command",
      icon: "gear" as IconName,
      run: () => openDialog({ kind: "settings" }),
    },
  ];
  if (!desktop.status?.notUpdating) {
    commands.push({
      id: "check-updates",
      title: "Check for updates",
      detail: "Reads the newest release on GitHub",
      icon: "import" as IconName,
      run: () => void checkNow(),
    });
  }
  return commands;
}
