// The colour theme button cycles system, light, then dark, like the website's.

export type Theme = "system" | "light" | "dark";

const ORDER: Theme[] = ["system", "light", "dark"];

const NAMES: Record<Theme, string> = {
  system: "system",
  light: "light",
  dark: "dark",
};

/** A stored value as a theme; anything unknown follows the system. */
export function parseTheme(value: string | null | undefined): Theme {
  return value === "light" || value === "dark" ? value : "system";
}

/** The theme after this one in the cycle. */
export function nextTheme(theme: Theme): Theme {
  return ORDER[(ORDER.indexOf(theme) + 1) % ORDER.length] ?? "system";
}

/** Says which theme is active and what a click switches to. */
export function themeLabel(theme: Theme): string {
  return `Theme: ${NAMES[theme]}. Click for ${NAMES[nextTheme(theme)]}`;
}
