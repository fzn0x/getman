<script lang="ts">
  import { dialogs } from "../../lib/store.svelte";
  import CodeDialog from "./CodeDialog.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import CookiesDialog from "./CookiesDialog.svelte";
  import HeaderPresetsDialog from "./HeaderPresetsDialog.svelte";
  import ImportDialog from "./ImportDialog.svelte";
  import MoveDialog from "./MoveDialog.svelte";
  import Palette from "./Palette.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import SaveAsDialog from "./SaveAsDialog.svelte";
  import SetVariableDialog from "./SetVariableDialog.svelte";
  import ShortcutsDialog from "./ShortcutsDialog.svelte";

  const current = $derived(dialogs.current);
</script>

{#if current}
  {#key current}
    {#if current.kind === "palette"}
      <Palette />
    {:else if current.kind === "import"}
      <ImportDialog folder={current.folder} />
    {:else if current.kind === "code"}
      <CodeDialog tab={current.tab} />
    {:else if current.kind === "shortcuts"}
      <ShortcutsDialog />
    {:else if current.kind === "cookies"}
      <CookiesDialog />
    {:else if current.kind === "confirm"}
      <ConfirmDialog dialog={current} />
    {:else if current.kind === "prompt"}
      <PromptDialog dialog={current} />
    {:else if current.kind === "saveAs"}
      <SaveAsDialog tab={current.tab} />
    {:else if current.kind === "move"}
      <MoveDialog paths={current.paths} />
    {:else if current.kind === "headerPresets"}
      <HeaderPresetsDialog />
    {:else if current.kind === "setVariable"}
      <SetVariableDialog value={current.value} name={current.name} tab={current.tab} />
    {/if}
  {/key}
{/if}
