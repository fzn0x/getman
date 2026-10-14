// The site's few behaviours. Every page reads without them.
(() => {
  "use strict";

  const root = document.documentElement;
  const mac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
  const reduced = matchMedia("(prefers-reduced-motion: reduce)");
  const base = new URL(document.querySelector('link[rel="stylesheet"]').href).href.replace(
    /site\.css.*$/,
    "",
  );

  // ---- Theme: system, then light, then dark --------------------------------------

  const themeButton = document.querySelector("[data-theme-toggle]");
  const themeLabels = {
    system: "Colour theme: follows your system",
    light: "Colour theme: light",
    dark: "Colour theme: dark",
  };
  function setTheme(theme) {
    if (theme === "system") delete root.dataset.theme;
    else root.dataset.theme = theme;
    try {
      if (theme === "system") localStorage.removeItem("getman.theme");
      else localStorage.setItem("getman.theme", theme);
    } catch {}
    themeButton?.setAttribute("aria-label", themeLabels[theme]);
    themeButton?.setAttribute("title", themeLabels[theme]);
    drawAll();
  }
  if (themeButton) {
    const current = root.dataset.theme || "system";
    themeButton.setAttribute("aria-label", themeLabels[current]);
    themeButton.setAttribute("title", themeLabels[current]);
    themeButton.addEventListener("click", () => {
      const order = ["system", "light", "dark"];
      setTheme(order[(order.indexOf(root.dataset.theme || "system") + 1) % 3]);
    });
  }

  // ---- Copying -------------------------------------------------------------------

  async function copy(text, button, label) {
    try {
      await navigator.clipboard.writeText(text);
      flash(button, "Copied");
    } catch {
      flash(button, "Select and copy");
    }
    function flash(target, message) {
      const span = target.querySelector("span") || target;
      const before = label ?? span.textContent;
      span.textContent = message;
      clearTimeout(target._timer);
      target._timer = setTimeout(() => (span.textContent = before), 1600);
    }
  }

  document.addEventListener("click", (event) => {
    const button = event.target.closest("[data-copy]");
    if (button) {
      const lines = (button.closest("figure")?.querySelector("pre")?.innerText ?? "").split("\n");
      // In a terminal session, copy only the commands, without the prompt,
      // with the lines a trailing backslash continues them onto.
      const commands = [];
      for (let i = 0; i < lines.length; i++) {
        if (!lines[i].startsWith("$ ")) continue;
        commands.push(lines[i].slice(2));
        while (lines[i].endsWith("\\") && i + 1 < lines.length) commands.push(lines[++i]);
      }
      const text = (commands.length ? commands : lines).join("\n");
      copy(text.trim(), button, "Copy");
      return;
    }
    const command = event.target.closest("[data-copy-command]");
    if (command) {
      const text = command.closest("[data-install]").querySelector("[data-command]").textContent;
      copy(text, command, "Copy");
    }
  });

  // ---- The install command for this system ----------------------------------------

  const installs = [...document.querySelectorAll("[data-install]")];
  function setSystem(system) {
    for (const install of installs) {
      const select = install.querySelector("[data-os]");
      const command = install.querySelector("[data-command]");
      select.value = system;
      // Narrow screens may wrap the command, but only after a slash.
      command.replaceChildren();
      command.dataset[system].split("/").forEach((part, i, parts) => {
        command.append(part + (i < parts.length - 1 ? "/" : ""));
        if (i < parts.length - 1) command.append(document.createElement("wbr"));
      });
    }
  }
  if (installs.length) {
    if (/Win/.test(navigator.platform || navigator.userAgent)) setSystem("windows");
    for (const install of installs) {
      install
        .querySelector("[data-os]")
        .addEventListener("change", (event) => setSystem(event.target.value));
    }
  }

  // ---- Keys say Cmd on a Mac -----------------------------------------------------

  if (mac) for (const key of document.querySelectorAll("[data-mod]")) key.textContent = "⌘";

  // ---- Halftone: dots grow with the response time, as in the app -------------------

  const STEP = 6;
  function draw(svg) {
    const box = svg.getBoundingClientRect();
    const width = Math.ceil(box.width);
    const height = Math.ceil(box.height);
    if (!width || !height) return;
    const host = svg.closest("[data-time]") ?? svg;
    const time = Number(host.dataset.time || svg.dataset.time || 500);
    const darkness = Math.min(1, Math.max(0, Math.log10(Math.max(time, 1)) / 4));
    const columns = Math.ceil(width / STEP);
    const rows = Math.ceil(height / STEP);
    const parts = [];
    for (let c = 0; c < columns; c++) {
      const across = c / Math.max(columns - 1, 1);
      for (let r = 0; r < rows; r++) {
        const down = r / Math.max(rows - 1, 1);
        const falloff = 1 - 0.75 * across + 0.15 * (down - 0.5);
        const radius = Math.min(
          STEP / 2,
          Math.max(0.35, (0.4 + 2.4 * darkness) * Math.max(falloff, 0.12)),
        );
        parts.push(
          `<circle cx="${c * STEP + STEP / 2}" cy="${r * STEP + STEP / 2}" r="${radius.toFixed(2)}"/>`,
        );
      }
    }
    svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
    svg.innerHTML = parts.join("");
  }
  const fields = [...document.querySelectorAll("svg.halftone")];
  function drawAll() {
    fields.forEach(draw);
  }
  if (fields.length) {
    drawAll();
    const observer = new ResizeObserver(() => drawAll());
    fields.forEach((svg) => observer.observe(svg.parentElement));
  }

  // ---- The plate: pick a request, watch it send -----------------------------------

  const tabs = [...document.querySelectorAll('.plate-tabs [role="tab"]')];
  const panel = document.getElementById("example-panel");
  function show(index, focus) {
    const template = document.querySelector(`template[data-example="${index}"]`);
    if (!template || !panel) return;
    tabs.forEach((tab, i) => {
      tab.setAttribute("aria-selected", String(i === index));
      tab.tabIndex = i === index ? 0 : -1;
    });
    if (focus) tabs[index].focus();
    panel.setAttribute("aria-labelledby", tabs[index].id);
    const finish = () => {
      panel.querySelector(".plate-content").replaceChildren(template.content.cloneNode(true));
      panel.className = `plate-body tone-${template.dataset.tone}`;
      panel.dataset.time = template.dataset.time;
      draw(panel.querySelector(".halftone"));
    };
    if (reduced.matches) {
      finish();
      return;
    }
    panel.className = "plate-body sending";
    panel.dataset.time = "200";
    draw(panel.querySelector(".halftone"));
    clearTimeout(panel._timer);
    panel._timer = setTimeout(finish, 600);
  }
  tabs.forEach((tab, i) => {
    tab.addEventListener("click", () => show(i, false));
    tab.addEventListener("keydown", (event) => {
      const moves = { ArrowRight: 1, ArrowLeft: -1, Home: -i, End: tabs.length - 1 - i };
      if (!(event.key in moves)) return;
      event.preventDefault();
      show((i + moves[event.key] + tabs.length) % tabs.length, true);
    });
  });

  // ---- Tab rows that overflow fade at the cut edge, as in the app ----------------------

  for (const row of document.querySelectorAll(".plate-tabs")) {
    const update = () => {
      row.classList.toggle("fade-start", row.scrollLeft > 1);
      row.classList.toggle("fade-end", row.scrollLeft + row.clientWidth < row.scrollWidth - 1);
    };
    row.addEventListener("scroll", update, { passive: true });
    new ResizeObserver(update).observe(row);
    update();
  }

  // ---- The documentation menu on small screens -------------------------------------

  const sidebar = document.querySelector(".sidebar");
  const toggle = sidebar?.querySelector(".sidebar-toggle");
  toggle?.addEventListener("click", () => {
    const open = sidebar.classList.toggle("open");
    toggle.setAttribute("aria-expanded", String(open));
  });

  // ---- Where you are on the page --------------------------------------------------

  const tocLinks = [...document.querySelectorAll(".toc a")];
  if (tocLinks.length) {
    const targets = tocLinks.map((a) => document.getElementById(a.hash.slice(1))).filter(Boolean);
    const mark = () => {
      const line = 120;
      let current = targets[0];
      for (const target of targets)
        if (target.getBoundingClientRect().top <= line) current = target;
      tocLinks.forEach((a) => a.classList.toggle("here", a.hash === `#${current?.id}`));
    };
    addEventListener("scroll", mark, { passive: true });
    mark();
  }

  // ---- Search ---------------------------------------------------------------------

  const dialog = document.querySelector(".palette");
  const input = dialog?.querySelector("input");
  const list = dialog?.querySelector("ul");
  let index = null;
  let results = [];
  let selected = 0;

  function score(query, text) {
    // Every character of the query, in order; words and starts count more.
    const q = query.toLowerCase();
    const t = text.toLowerCase();
    if (t.includes(q)) return 100 - t.indexOf(q) + (t.startsWith(q) ? 50 : 0);
    let at = 0;
    let points = 0;
    for (const ch of q) {
      const found = t.indexOf(ch, at);
      if (found < 0) return -1;
      points += found === at ? 3 : found === 0 || t[found - 1] === " " ? 2 : 1;
      at = found + 1;
    }
    return points;
  }

  function render() {
    const query = input.value.trim();
    results = !query
      ? index.filter((item) => item.kind === "page")
      : index
          .map((item) => ({ item, s: score(query, item.title) }))
          .filter((r) => r.s >= 0)
          .sort((a, b) => b.s - a.s || (a.item.kind === "page" ? -1 : 1))
          .slice(0, 30)
          .map((r) => r.item);
    selected = Math.min(selected, Math.max(results.length - 1, 0));
    list.replaceChildren(
      ...(results.length
        ? results.map((item, i) => {
            const li = document.createElement("li");
            li.id = `result-${i}`;
            li.setAttribute("role", "option");
            li.setAttribute("aria-selected", String(i === selected));
            const a = document.createElement("a");
            a.href = base + item.url;
            a.tabIndex = -1;
            const title = document.createElement("span");
            title.textContent = item.title;
            const kind = document.createElement("span");
            kind.className = "kind";
            kind.textContent = item.kind === "page" ? "Page" : item.kind;
            a.append(title, kind);
            li.append(a);
            return li;
          })
        : [
            Object.assign(document.createElement("li"), {
              className: "empty",
              textContent: `Nothing matches “${query}”.`,
            }),
          ]),
    );
    input.setAttribute("aria-activedescendant", results.length ? `result-${selected}` : "");
    list.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }

  async function openPalette() {
    if (!dialog) return;
    if (!index) {
      try {
        index = await (await fetch(`${base}search.json`)).json();
      } catch {
        index = [];
      }
    }
    if (!dialog.open) dialog.showModal();
    input.value = "";
    selected = 0;
    render();
    input.focus();
  }

  document.addEventListener("click", (event) => {
    if (event.target.closest("[data-palette]")) openPalette();
  });
  dialog?.addEventListener("click", (event) => {
    if (event.target === dialog) dialog.close();
  });
  input?.addEventListener("input", () => {
    selected = 0;
    render();
  });
  input?.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!results.length) return;
      selected =
        (selected + (event.key === "ArrowDown" ? 1 : -1) + results.length) % results.length;
      render();
    } else if (event.key === "Enter" && results[selected]) {
      event.preventDefault();
      location.href = base + results[selected].url;
      dialog.close();
    }
  });
  document.addEventListener("keydown", (event) => {
    const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement?.tagName ?? "");
    if ((event.key === "k" || event.key === "K") && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      openPalette();
    } else if (event.key === "/" && !typing && !dialog?.open) {
      event.preventDefault();
      openPalette();
    }
  });
})();
