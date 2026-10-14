<script lang="ts">
  import Editor from "../Editor.svelte";

  interface Props {
    scripts: { pre_request?: string; post_response?: string };
    /** Where the scripts run, for the explanation. */
    scope?: "request" | "folder" | "workspace";
  }

  let { scripts = $bindable(), scope = "request" }: Props = $props();
  let which = $state<"post_response" | "pre_request">(
    scripts.post_response || !scripts.pre_request ? "post_response" : "pre_request",
  );

  const snippets: Record<typeof which, { label: string; code: string }[]> = {
    pre_request: [
      { label: "Set a variable", code: 'gm.variables.set("requestId", crypto.randomUUID());' },
      {
        label: "Add a header",
        code: 'gm.request.headers.upsert({ key: "X-Request-Id", value: gm.variables.get("requestId") });',
      },
      { label: "Timestamp", code: 'gm.variables.set("now", new Date().toISOString());' },
      {
        label: "Sign with HMAC",
        code: 'const signature = gm.crypto.hmac("sha256", gm.environment.get("secret"), gm.request.body.raw ?? "", "base64");\ngm.request.headers.upsert({ key: "X-Signature", value: signature });',
      },
      {
        label: "Fetch a token first",
        code: 'const res = await gm.sendRequest({\n  url: gm.variables.replaceIn("{{baseUrl}}/login"),\n  method: "POST",\n  body: { mode: "raw", raw: JSON.stringify({ user: "ada" }) },\n});\ngm.environment.set("token", res.json().token);',
      },
    ],
    post_response: [
      {
        label: "Status is 200",
        code: 'gm.test("status is 200", () => {\n  gm.response.to.have.status(200);\n});',
      },
      {
        label: "Body has a field",
        code: 'gm.test("has an id", () => {\n  gm.expect(gm.response.json()).to.have.property("id");\n});',
      },
      {
        label: "Response time",
        code: 'gm.test("answers within 500 ms", () => {\n  gm.expect(gm.response.responseTime).to.be.below(500);\n});',
      },
      { label: "Save a value", code: 'gm.environment.set("token", gm.response.json().token);' },
      {
        label: "Match a schema",
        code: 'gm.test("matches the schema", () => {\n  gm.response.to.have.jsonSchema({\n    type: "object",\n    required: ["id"],\n  });\n});',
      },
      { label: "Log the body", code: "console.log(gm.response.json());" },
    ],
  };

  function insert(code: string): void {
    const current = scripts[which] ?? "";
    scripts[which] =
      current.trim() === "" ? `${code}\n` : `${current.replace(/\s*$/, "")}\n\n${code}\n`;
  }

  const where = $derived(
    scope === "request"
      ? "this request"
      : scope === "folder"
        ? "every request in this folder"
        : "every request in the workspace",
  );
</script>

<div class="scripts">
  <div class="tablist" role="tablist" aria-label="Script">
    <button
      class="tab"
      role="tab"
      aria-selected={which === "pre_request"}
      onclick={() => (which = "pre_request")}
    >
      Pre-request {#if scripts.pre_request?.trim()}<span class="has" aria-label="has a script"
        ></span>{/if}
    </button>
    <button
      class="tab"
      role="tab"
      aria-selected={which === "post_response"}
      onclick={() => (which = "post_response")}
    >
      Post-response {#if scripts.post_response?.trim()}<span class="has" aria-label="has a script"
        ></span>{/if}
    </button>
  </div>
  <div class="split">
    <div class="code">
      {#key which}
        <Editor
          bind:value={
            () => scripts[which] ?? "",
            (value) => {
              scripts[which] = value;
            }
          }
          language="javascript"
          label={which === "pre_request" ? "Pre-request script" : "Post-response script"}
          placeholder={which === "pre_request"
            ? "// Runs before the request is sent. Change it with gm.request."
            : '// Runs after the response arrives.\n// gm.test("status is 200", () => gm.response.to.have.status(200));'}
        />
      {/key}
    </div>
    <aside class="snippets" aria-label="Snippets">
      <p class="muted">
        Runs {which === "pre_request" ? "before" : "after"}
        {where}. <code>pm</code> works as
        <code>gm</code>.
      </p>
      {#each snippets[which] as snippet (snippet.label)}
        <button class="snippet" onclick={() => insert(snippet.code)}>{snippet.label}</button>
      {/each}
    </aside>
  </div>
</div>

<style>
  .scripts {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: var(--s2);
    height: 100%;
    min-height: 0;
  }

  .has {
    width: 5px;
    height: 5px;
    background: var(--ink-3);
  }

  .split {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 190px;
    gap: var(--s3);
    min-height: 0;
  }

  .code {
    min-height: 160px;
    border: 1px solid var(--rule-2);
    overflow: hidden;
  }

  .snippets {
    display: grid;
    align-content: start;
    gap: 2px;
    overflow: auto;
  }

  .snippets p {
    margin: 0 0 var(--s2);
    font-size: var(--fs-meta);
  }

  code {
    font-family: var(--font-code);
  }

  .snippet {
    padding: 5px var(--s2);
    border: 0;
    border-left: 1px solid var(--rule-2);
    background: none;
    text-align: left;
    color: var(--ink-2);
  }

  .snippet:hover {
    border-left-color: var(--ink);
    background: var(--sunken);
    color: var(--ink);
  }

  @media (max-width: 1100px) {
    .split {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(0, 1fr) auto;
    }

    .snippets {
      grid-auto-flow: column;
      grid-auto-columns: max-content;
      overflow-x: auto;
    }

    .snippets p {
      display: none;
    }
  }
</style>
