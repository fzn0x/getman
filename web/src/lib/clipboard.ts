import { notify } from "./store.svelte";

/** Copies text and says so. */
export async function copyText(text: string, what = "Copied"): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    notify(what, "ok");
  } catch {
    notify("The browser did not allow copying; select the text and copy it instead.", "error");
  }
}
