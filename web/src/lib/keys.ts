// Keyboard shortcuts. Browsers keep some keys for themselves (Ctrl+N,
// Ctrl+W, Ctrl+Tab), so tab and window actions use Alt instead.

export const isMac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** Whether a key event matches a combo such as `mod+enter` or `alt+]`. */
export function matches(event: KeyboardEvent, combo: string): boolean {
  const parts = combo.toLowerCase().split("+");
  const key = parts.pop() ?? "";
  const want = new Set(parts);
  const mod = isMac ? event.metaKey : event.ctrlKey;
  if (want.has("mod") !== mod) return false;
  if (want.has("alt") !== event.altKey) return false;
  if (want.has("shift") !== event.shiftKey) return false;
  if (!isMac && !want.has("mod") && event.ctrlKey) return false;
  if (isMac && !want.has("mod") && event.metaKey) return false;
  const pressed = event.key.toLowerCase();
  if (pressed === key) return true;
  // Alt changes the character on some layouts, and Option on a Mac types a
  // symbol for digits and brackets; fall back to the key's code.
  const code = event.code.toLowerCase();
  return (
    code === `key${key}` ||
    code === `digit${key}` ||
    (key === "[" && code === "bracketleft") ||
    (key === "]" && code === "bracketright") ||
    (key === "enter" && code === "numpadenter")
  );
}

/** The digit 1 to 9 of an Alt press, by the physical key, or null. */
export function altDigit(event: KeyboardEvent): number | null {
  if (!event.altKey || event.ctrlKey || event.metaKey) return null;
  const code = /^Digit([1-9])$/.exec(event.code ?? "");
  if (code) return Number(code[1]);
  return /^[1-9]$/.test(event.key) ? Number(event.key) : null;
}

/** A combo written for people: `Ctrl S`, `Cmd Enter`, `Alt ]`. */
export function keyLabel(combo: string): string[] {
  return combo.split("+").map((part) => {
    switch (part.toLowerCase()) {
      case "mod":
        return isMac ? "Cmd" : "Ctrl";
      case "alt":
        return isMac ? "Opt" : "Alt";
      case "shift":
        return "Shift";
      case "enter":
        return "Enter";
      case "escape":
        return "Esc";
      default:
        return part.toUpperCase();
    }
  });
}

/** Whether the event comes from a place where typing is expected. */
export function typing(event: KeyboardEvent): boolean {
  const target = event.target as HTMLElement | null;
  if (!target) return false;
  return (
    target.isContentEditable ||
    target.closest(".cm-editor") !== null ||
    ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)
  );
}

export const SHORTCUTS: { combo: string; action: string }[] = [
  { combo: "mod+enter", action: "Send the request" },
  { combo: "escape", action: "Cancel the request being sent" },
  { combo: "mod+s", action: "Save" },
  { combo: "mod+k", action: "Find requests and commands" },
  { combo: "alt+n", action: "New request" },
  { combo: "alt+w", action: "Close the tab" },
  { combo: "alt+shift+t", action: "Reopen the tab closed last" },
  { combo: "alt+a", action: "Search open and recently closed tabs" },
  { combo: "alt+]", action: "Next tab" },
  { combo: "alt+[", action: "Previous tab" },
  { combo: "alt+l", action: "Edit the URL" },
  { combo: "alt+e", action: "Switch environment" },
  { combo: "alt+c", action: "Copy the request as code" },
  { combo: "mod+b", action: "Show or hide the sidebar" },
  { combo: "alt+i", action: "Import" },
  { combo: "mod+j", action: "Show or hide the console" },
  { combo: "mod+z", action: "Undo a delete, rename or move, outside a text field" },
  { combo: "mod+shift+z", action: "Redo, outside a text field" },
  { combo: "?", action: "Show these shortcuts" },
];
