<script lang="ts">
  import { onMount } from "svelte";
  import DialogHost from "./components/dialogs/DialogHost.svelte";
  import EnvironmentsView from "./components/EnvironmentsView.svelte";
  import FolderView from "./components/FolderView.svelte";
  import Notices from "./components/Notices.svelte";
  import RequestView from "./components/request/RequestView.svelte";
  import RunnerView from "./components/RunnerView.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import SignedOut from "./components/SignedOut.svelte";
  import TabStrip from "./components/TabStrip.svelte";
  import TopBar from "./components/TopBar.svelte";
  import Welcome from "./components/Welcome.svelte";
  import { whenSignedOut } from "./lib/api";
  import { matches, typing } from "./lib/keys";
  import {
    activeTab,
    closeTab,
    cycleTab,
    dialogs,
    isDirty,
    loadWorkspace,
    newRequest,
    openDialog,
    prefs,
    refreshTree,
    save,
    selectTab,
    send,
    setTheme,
    signals,
    store,
    tabs,
    ws,
  } from "./lib/store.svelte";

  whenSignedOut(() => (ws.signedOut = true));

  const tab = $derived(activeTab());
  let narrow = $state(false);
  let resizing = $state(false);

  onMount(() => {
    setTheme(prefs.theme);
    void loadWorkspace();
    const media = matchMedia("(max-width: 900px)");
    const update = () => (narrow = media.matches);
    update();
    // On a phone the sidebar covers the page, so start with it closed.
    if (narrow) prefs.sidebarOpen = false;
    media.addEventListener("change", update);
    // Files change underneath the UI (an editor, git checkout); catch up
    // whenever the window comes back.
    const onFocus = () => {
      if (ws.ready) void refreshTree();
    };
    window.addEventListener("focus", onFocus);
    const beforeUnload = (event: BeforeUnloadEvent) => {
      if (tabs.list.some(isDirty)) event.preventDefault();
    };
    window.addEventListener("beforeunload", beforeUnload);
    return () => {
      media.removeEventListener("change", update);
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("beforeunload", beforeUnload);
    };
  });

  $effect(() => {
    store("layout", prefs.layout);
    store("sidebarOpen", prefs.sidebarOpen);
    store("sidebarWidth", prefs.sidebarWidth);
    store("split", prefs.split);
  });

  function onKeydown(event: KeyboardEvent): void {
    if (!ws.ready || dialogs.current) return;
    const current = activeTab();
    const handle = (action: () => void) => {
      event.preventDefault();
      action();
    };
    if (matches(event, "mod+enter")) {
      if (current?.kind === "request") handle(() => void send(current));
    } else if (matches(event, "mod+s")) {
      handle(() => void save(current));
    } else if (matches(event, "mod+k") || matches(event, "mod+p")) {
      handle(() => openDialog({ kind: "palette" }));
    } else if (matches(event, "alt+n")) {
      handle(() => newRequest());
    } else if (matches(event, "alt+w")) {
      if (current) handle(() => closeTab(current.id));
    } else if (matches(event, "alt+]")) {
      handle(() => cycleTab(1));
    } else if (matches(event, "alt+[")) {
      handle(() => cycleTab(-1));
    } else if (matches(event, "alt+l")) {
      handle(() => signals.focusUrl++);
    } else if (matches(event, "alt+e")) {
      handle(() => signals.focusEnvironment++);
    } else if (matches(event, "alt+c")) {
      if (current?.kind === "request") handle(() => openDialog({ kind: "code", tab: current }));
    } else if (matches(event, "alt+i")) {
      handle(() => openDialog({ kind: "import", folder: "" }));
    } else if (matches(event, "mod+b")) {
      handle(() => (prefs.sidebarOpen = !prefs.sidebarOpen));
    } else if (/^[1-9]$/.test(event.key) && event.altKey && !event.ctrlKey && !event.metaKey) {
      const target = tabs.list[Number(event.key) - 1];
      if (target) handle(() => selectTab(target.id));
    } else if (event.key === "?" && !typing(event)) {
      handle(() => openDialog({ kind: "shortcuts" }));
    } else if (event.key === "/" && !typing(event)) {
      handle(() => {
        prefs.sidebarOpen = true;
        signals.focusFilter++;
      });
    }
  }

  function startResize(event: PointerEvent): void {
    const handle = event.currentTarget as HTMLElement;
    handle.setPointerCapture(event.pointerId);
    resizing = true;
    const move = (e: PointerEvent) => {
      prefs.sidebarWidth = Math.round(Math.min(520, Math.max(200, e.clientX)));
    };
    const up = () => {
      resizing = false;
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
  }

  function resizeWithKeys(event: KeyboardEvent): void {
    if (event.key === "ArrowLeft") prefs.sidebarWidth = Math.max(200, prefs.sidebarWidth - 16);
    else if (event.key === "ArrowRight")
      prefs.sidebarWidth = Math.min(520, prefs.sidebarWidth + 16);
  }

  const requestCount = $derived.by(() => {
    const count = (nodes: typeof ws.tree): number =>
      nodes.reduce((sum, n) => sum + (n.kind === "folder" ? count(n.children) : 1), 0);
    return count(ws.tree);
  });
</script>

<svelte:window onkeydown={onKeydown} />

{#if ws.signedOut}
  <SignedOut />
{:else if !ws.ready}
  <div class="boot" aria-busy="true">
    {#if ws.failure}
      <div class="empty">
        <h2>Not connected</h2>
        <p>{ws.failure}</p>
        <button class="btn" onclick={() => void loadWorkspace()}>Try again</button>
      </div>
    {:else}
      <p class="bitmap boot-mark">Getman</p>
    {/if}
  </div>
{:else}
  <div
    class="app"
    class:narrow
    class:no-sidebar={!prefs.sidebarOpen}
    class:resizing
    style:--sidebar-width="{prefs.sidebarWidth}px"
  >
    <TopBar />
    {#if prefs.sidebarOpen}
      <div class="side">
        <Sidebar />
      </div>
      {#if !narrow}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
        <div
          class="resizer"
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize the sidebar"
          aria-valuenow={prefs.sidebarWidth}
          aria-valuemin={200}
          aria-valuemax={520}
          tabindex="0"
          onpointerdown={startResize}
          onkeydown={resizeWithKeys}
        ></div>
      {:else}
        <button
          class="scrim"
          aria-label="Close the sidebar"
          onclick={() => (prefs.sidebarOpen = false)}
        ></button>
      {/if}
    {/if}
    <main class="main">
      <TabStrip />
      <div class="view">
        {#if !tab}
          <Welcome />
        {:else if tab.kind === "request"}
          {#key tab.id}
            <RequestView {tab} />
          {/key}
        {:else if tab.kind === "folder"}
          {#key tab.id}
            <FolderView {tab} />
          {/key}
        {:else if tab.kind === "environments"}
          <EnvironmentsView {tab} />
        {:else if tab.kind === "runner"}
          {#key tab.id}
            <RunnerView {tab} />
          {/key}
        {/if}
      </div>
    </main>
    <footer class="colophon bitmap">
      <span>Getman {ws.version}</span>
      <span class="sep" aria-hidden="true"></span>
      <span class="root" title={ws.root}>{ws.root}</span>
      <span class="sep" aria-hidden="true"></span>
      <span>{requestCount} requests</span>
      <span class="fill"></span>
      <button class="link bitmap" onclick={() => openDialog({ kind: "shortcuts" })}>
        Shortcuts ?
      </button>
    </footer>
  </div>
{/if}

<DialogHost />
<Notices />

<style>
  .boot {
    display: grid;
    place-items: center;
    height: 100%;
  }

  .boot-mark {
    font-size: var(--b4);
    margin: 0;
    animation: blink 1.2s steps(2, jump-none) infinite;
  }

  @keyframes blink {
    50% {
      opacity: 0.25;
    }
  }

  .app {
    display: grid;
    grid-template-columns: var(--sidebar-width) 1px minmax(0, 1fr);
    grid-template-rows: var(--bar) minmax(0, 1fr) 24px;
    grid-template-areas:
      "top top top"
      "side handle main"
      "foot foot foot";
    height: 100%;
  }

  .app.no-sidebar {
    grid-template-columns: 0 0 minmax(0, 1fr);
  }

  .app.resizing {
    cursor: col-resize;
    user-select: none;
  }

  .side {
    grid-area: side;
    min-width: 0;
    min-height: 0;
    border-right: 1px solid var(--rule);
    background: var(--paper);
  }

  .resizer {
    grid-area: handle;
    position: relative;
    z-index: 2;
    cursor: col-resize;
  }

  .resizer::before {
    content: "";
    position: absolute;
    inset: 0 -4px;
  }

  .resizer:hover::after,
  .resizer:focus-visible::after {
    content: "";
    position: absolute;
    inset: 0 -1px;
    background: var(--accent);
  }

  .main {
    grid-area: main;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
  }

  .view {
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }

  .colophon {
    grid-area: foot;
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: 0 var(--s3);
    border-top: 1px solid var(--rule);
    color: var(--ink-3);
    white-space: nowrap;
    overflow: hidden;
  }

  .colophon .root {
    overflow: hidden;
    text-overflow: ellipsis;
    text-transform: none;
  }

  .colophon .sep {
    width: 4px;
    height: 4px;
    background: var(--ink-3);
    flex: none;
  }

  .colophon .fill {
    flex: 1;
  }

  .link {
    border: 0;
    background: none;
    padding: 0;
    color: var(--ink-3);
  }

  .link:hover {
    color: var(--ink);
  }

  .app.narrow {
    grid-template-columns: 0 0 minmax(0, 1fr);
  }

  .app.narrow .side {
    position: fixed;
    z-index: 20;
    top: var(--bar);
    bottom: 24px;
    left: 0;
    width: min(320px, 86vw);
  }

  .scrim {
    position: fixed;
    z-index: 19;
    inset: var(--bar) 0 24px;
    border: 0;
    background: color-mix(in srgb, var(--ink) 30%, transparent);
  }
</style>
