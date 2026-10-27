<script lang="ts">
  // A password-style field with a Show button, for secrets and keys.

  interface Props {
    value: string | undefined;
    placeholder?: string;
    label?: string;
  }

  let { value = $bindable(), placeholder = "", label = "" }: Props = $props();
  let reveal = $state(false);
</script>

<span class="secret">
  <input
    class="input mono"
    type={reveal ? "text" : "password"}
    value={value ?? ""}
    oninput={(event) => (value = event.currentTarget.value)}
    {placeholder}
    aria-label={label || undefined}
    spellcheck="false"
    autocomplete="off"
  />
  <button class="btn quiet" type="button" onclick={() => (reveal = !reveal)}
    >{reveal ? "Hide" : "Show"}</button
  >
</span>

<style>
  .secret {
    display: flex;
    gap: var(--s1);
  }

  .secret .input {
    flex: 1;
  }
</style>
