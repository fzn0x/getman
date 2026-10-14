<script lang="ts">
  import type { KeyValue, Vars } from "../../lib/types";
  import KeyValueTable from "./KeyValueTable.svelte";

  interface Props {
    vars: Vars;
    onChange: (vars: Vars) => void;
    label: string;
    nameLabel?: string;
    valueLabel?: string;
  }

  let { vars, onChange, label, nameLabel = "Variable", valueLabel = "Value" }: Props = $props();

  const rows = $derived<KeyValue[]>(
    Object.entries(vars).map(([name, value]) => ({ name, value, enabled: true })),
  );
</script>

<KeyValueTable
  {rows}
  onChange={(next) => onChange(Object.fromEntries(next.map((row) => [row.name, row.value])))}
  toggles={false}
  {label}
  {nameLabel}
  {valueLabel}
/>
