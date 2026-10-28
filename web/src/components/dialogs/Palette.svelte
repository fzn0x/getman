<script lang="ts">
  import { tick } from "svelte";
  import { newGrpcDraft, toRequest } from "../../lib/draft";
  import { desktopCommands } from "../../lib/desktop.svelte";
  import { methodClass } from "../../lib/format";
  import { rank } from "../../lib/fuzzy";
  import { KIND_LABELS, kindMark } from "../../lib/realtime";
  import type { IconName } from "../../lib/icons";
  import { keyLabel } from "../../lib/keys";
  import {
    activeTab,
    allFolders,
    allRequests,
    clearSession,
    closeDialog,
    newRequest,
    openDialog,
    openEnvironments,
    openFolder,
    openRequest,
    openRunner,
    prefs,
    selectEnvironment,
    setTheme,
    ws,
  } from "../../lib/store.svelte";
  import Icon from "../Icon.svelte";
  import Dialog from "./Dialog.svelte";

  interface Entry {
    id: string;
    title: string;
    detail?: string;
    method?: string;
    icon?: IconName;
    keys?: string;
    run: () => void;
  }

  let query = $state("");
  let index = $state(0);
  let list: HTMLUListElement;

  const commands = $derived.by((): Entry[] => {
    const tab = activeTab();
    const entries: Entry[] = [
      { id: "new", title: "New request", icon: "plus", keys: "alt+n", run: () => newRequest() },
      ...(["websocket", "socketio", "sse"] as const).map((kind) => ({
        id: `new-${kind}`,
        title: `New ${KIND_LABELS[kind]} request`,
        icon: "plus" as IconName,
        run: () =>
          newRequest({ name: `Untitled ${KIND_LABELS[kind]}`, kind, method: "GET", url: "" }),
      })),
      {
        id: "new-grpc",
        title: "New gRPC request",
        detail: "Call a gRPC method, from .proto files or server reflection",
        icon: "plus",
        run: () => newRequest(toRequest(newGrpcDraft())),
      },
      {
        id: "import",
        title: "Import a collection, API spec, HAR file or cURL command",
        icon: "import",
        keys: "alt+i",
        run: () => openDialog({ kind: "import", folder: "" }),
      },
      {
        id: "export",
        title: "Export the workspace",
        detail: "As a Postman collection or an OpenAPI document",
        icon: "export",
        run: () => openDialog({ kind: "export", target: "folder", path: "", name: ws.name }),
      },
      { id: "run", title: "Run the whole workspace", icon: "play", run: () => openRunner("") },
      {
        id: "workspace",
        title: "Workspace settings",
        detail: "Variables, headers, auth and scripts for every request",
        icon: "gear",
        run: () => void openFolder(""),
      },
      {
        id: "environments",
        title: "Manage environments",
        icon: "globe",
        run: () => openEnvironments(ws.environment),
      },
      {
        id: "clear-session",
        title: "Clear session variables",
        detail: "The values scripts keep with gm.session until Getman stops",
        icon: "close",
        run: () => void clearSession(),
      },
      {
        id: "cookies",
        title: "Show cookies",
        icon: "cookie",
        run: () => openDialog({ kind: "cookies" }),
      },
      {
        id: "theme",
        title: "Switch light and dark theme",
        icon: "contrast",
        run: () =>
          setTheme(
            document.documentElement.dataset.theme === "dark" ||
              (prefs.theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches)
              ? "light"
              : "dark",
          ),
      },
      {
        id: "layout",
        title:
          prefs.layout === "columns"
            ? "Put the response below the request"
            : "Put the response beside the request",
        icon: prefs.layout === "columns" ? "stack" : "split",
        run: () => (prefs.layout = prefs.layout === "columns" ? "rows" : "columns"),
      },
      {
        id: "shortcuts",
        title: "Keyboard shortcuts",
        icon: "keyboard",
        keys: "?",
        run: () => openDialog({ kind: "shortcuts" }),
      },
      ...desktopCommands(),
      {
        id: "env-none",
        title: "Use no environment",
        icon: "globe",
        run: () => selectEnvironment(""),
      },
      ...ws.environments.map((environment) => ({
        id: `env-${environment.id}`,
        title: `Use environment: ${environment.name}`,
        icon: "globe" as IconName,
        run: () => selectEnvironment(environment.id),
      })),
    ];
    if (tab?.kind === "request") {
      entries.unshift({
        id: "code",
        title: "Copy this request as code",
        icon: "code",
        keys: "alt+c",
        run: () => openDialog({ kind: "code", tab }),
      });
    }
    return entries;
  });

  const requests = $derived(
    allRequests().map((request) => ({
      id: `r:${request.path}`,
      title: request.name,
      detail: request.path,
      method: kindMark(request.request_kind) || request.method,
      run: () => void openRequest(request.path),
    })),
  );

  const folders = $derived(
    allFolders().flatMap((folder) => [
      {
        id: `f:${folder.path}`,
        title: `Folder settings: ${folder.name}`,
        detail: folder.path,
        icon: "folder" as IconName,
        run: () => void openFolder(folder.path),
      },
      {
        id: `run:${folder.path}`,
        title: `Run folder: ${folder.name}`,
        detail: folder.path,
        icon: "play" as IconName,
        run: () => openRunner(folder.path),
      },
    ]),
  );

  const results = $derived.by((): Entry[] => {
    if (query.trim() === "") {
      const recent: Entry[] = [];
      for (const entry of ws.history) {
        const request = requests.find((r) => r.detail === entry.path);
        if (request && !recent.some((r) => r.id === request.id)) recent.push(request);
        if (recent.length === 5) break;
      }
      return [...recent, ...commands.slice(0, 8)];
    }
    if (query.startsWith(">")) return rank(query.slice(1), commands, (e) => e.title).slice(0, 40);
    return rank(
      query,
      [...requests, ...folders, ...commands],
      (e) => `${e.title} ${e.detail ?? ""}`,
    ).slice(0, 40);
  });

  $effect(() => {
    void results.length;
    index = 0;
  });

  async function move(step: number): Promise<void> {
    if (results.length === 0) return;
    index = (index + step + results.length) % results.length;
    await tick();
    list?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function choose(entry: Entry | undefined): void {
    if (!entry) return;
    closeDialog();
    entry.run();
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      void move(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      void move(-1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      choose(results[index]);
    }
  }
</script>

<Dialog title="Find requests and commands" width="640px" top>
  <div class="palette">
    <label class="search">
      <Icon name="search" />
      <span class="sr-only">Search</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        autofocus
        bind:value={query}
        onkeydown={onKeydown}
        placeholder="Find a request, or type > for commands"
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-results"
        aria-activedescendant={results[index] ? `palette-${index}` : undefined}
        spellcheck="false"
        autocomplete="off"
      />
    </label>
    <ul id="palette-results" role="listbox" bind:this={list}>
      {#each results as entry, i (entry.id)}
        <li
          id="palette-{i}"
          role="option"
          aria-selected={i === index}
          data-index={i}
          class:on={i === index}
          onmousemove={() => (index = i)}
          onclick={() => choose(entry)}
          onkeydown={() => {}}
        >
          <span class="lead">
            {#if entry.method}
              <span class="method {methodClass(entry.method)}">{entry.method}</span>
            {:else if entry.icon}
              <Icon name={entry.icon} />
            {/if}
          </span>
          <span class="text">
            <span class="title">{entry.title}</span>
            {#if entry.detail}<span class="detail">{entry.detail}</span>{/if}
          </span>
          {#if entry.keys}
            <span class="keys"
              >{#each keyLabel(entry.keys) as key (key)}<kbd>{key}</kbd>{/each}</span
            >
          {/if}
        </li>
      {:else}
        <li class="none" role="option" aria-selected="false">Nothing matches.</li>
      {/each}
    </ul>
  </div>
</Dialog>

<style>
  .palette {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-height: 0;
    max-height: inherit;
  }

  .search {
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: 0 var(--s4);
    border-bottom: 1px solid var(--rule);
  }

  .search:focus-within {
    border-bottom-color: var(--accent);
  }

  .search input {
    outline: none;
    flex: 1;
    height: 48px;
    border: 0;
    background: none;
    font-size: var(--fs-lead);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: var(--s1) 0;
    overflow: auto;
  }

  li {
    display: grid;
    grid-template-columns: 56px minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--s2);
    padding: 7px var(--s4);
    cursor: pointer;
  }

  li.on {
    background: var(--ink);
    color: var(--paper);
  }

  li.on .method,
  li.on .detail {
    color: inherit;
  }

  .lead {
    display: flex;
    justify-content: flex-start;
  }

  .text {
    display: grid;
    min-width: 0;
  }

  .title,
  .detail {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .detail {
    font-size: var(--fs-meta);
    color: var(--ink-3);
  }

  .keys {
    display: flex;
    gap: 2px;
  }

  .none {
    display: block;
    color: var(--ink-3);
    cursor: default;
  }
</style>
