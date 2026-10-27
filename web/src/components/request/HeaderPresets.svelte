<script lang="ts">
  import { api } from "../../lib/api";
  import { applyPreset, BUILT_IN_PRESETS, presetHeaders, withPreset } from "../../lib/presets";
  import { message, notify, openDialog, promptDialog } from "../../lib/store.svelte";
  import type { HeaderPreset, KeyValue } from "../../lib/types";
  import Icon from "../Icon.svelte";
  import Menu, { type MenuItem } from "../Menu.svelte";

  interface Props {
    rows: KeyValue[];
    onChange: (rows: KeyValue[]) => void;
  }

  let { rows, onChange }: Props = $props();
  let menu = $state<{ x: number; y: number; items: (MenuItem | "separator")[] } | null>(null);

  function describe(preset: HeaderPreset): string {
    return preset.headers.map((h) => h.name).join(", ");
  }

  function apply(preset: HeaderPreset): void {
    onChange(applyPreset(rows, preset.headers));
    notify(`Added ${preset.name} headers`, "ok");
  }

  function saveAsPreset(own: HeaderPreset[]): void {
    const headers = presetHeaders(rows);
    promptDialog({
      title: "Save headers as a preset",
      label: `Name for ${headers.map((h) => h.name).join(", ")}`,
      confirm: "Save preset",
      onSubmit: async (name) => {
        try {
          await api.writeHeaderPresets(withPreset(own, { name, headers }));
          notify(`Saved the ${name} preset`, "ok");
        } catch (error) {
          notify(message(error), "error");
        }
      },
    });
  }

  async function open(event: MouseEvent): Promise<void> {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    let own: HeaderPreset[] = [];
    try {
      own = await api.headerPresets();
    } catch (error) {
      notify(message(error), "error");
    }
    const item = (preset: HeaderPreset): MenuItem => ({
      label: `${preset.name}: ${describe(preset)}`,
      action: () => apply(preset),
    });
    menu = {
      x: rect.left,
      y: rect.bottom + 4,
      items: [
        ...BUILT_IN_PRESETS.map(item),
        ...(own.length ? (["separator", ...own.map(item)] as (MenuItem | "separator")[]) : []),
        "separator",
        {
          label: "Save these headers as a preset…",
          icon: "save",
          disabled: presetHeaders(rows).length === 0,
          action: () => saveAsPreset(own),
        },
        {
          label: "Manage presets…",
          icon: "gear",
          disabled: own.length === 0,
          action: () => openDialog({ kind: "headerPresets" }),
        },
      ],
    };
  }
</script>

<button
  class="btn quiet"
  aria-haspopup="menu"
  title="Add a set of headers, such as the ones a JSON API wants"
  onclick={(event) => void open(event)}
>
  <Icon name="stack" /> Presets
</button>

{#if menu}
  <Menu
    items={menu.items}
    x={menu.x}
    y={menu.y}
    label="Header presets"
    onClose={() => (menu = null)}
  />
{/if}
