// The site's few behaviours. Every page reads without them.
(() => {
  "use strict";

  const root = document.documentElement;
  const mac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
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
      // Only the command for the visitor's system is shown.
      const shown = [...command.parentElement.querySelectorAll("[data-command]")].find(
        (code) => code.getClientRects().length,
      );
      if (shown) copy(shown.textContent, command, "Copy");
    }
  });

  // ---- Keys say Cmd on a Mac -----------------------------------------------------

  if (mac) for (const key of document.querySelectorAll("[data-mod]")) key.textContent = "⌘";

  // ---- Feature mockups on the landing page ----------------------------------------
  // Each card replays its mockup while it is on screen: one gentle loop with
  // a hold on the end state. Without script, or with reduced motion, the
  // cards show their end state and nothing moves.

  const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;

  function countUp(el, ms = 900) {
    const to = Number(el.dataset.to);
    const decimals = String(el.dataset.to).split(".")[1]?.length ?? 0;
    if (reduce) {
      el.textContent = to.toFixed(decimals);
      return;
    }
    const t0 = performance.now();
    const tick = (t) => {
      const p = Math.min((t - t0) / ms, 1);
      el.textContent = (to * (1 - Math.pow(1 - p, 3))).toFixed(decimals);
      if (p < 1) requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }

  function typeOut(el, timers, step = 32) {
    const text = el.dataset.text;
    el.textContent = "";
    [...text].forEach((_, i) =>
      timers.push(setTimeout(() => (el.textContent = text.slice(0, i + 1)), i * step)),
    );
    return text.length * step;
  }

  const demos = {
    files: { cycle: 7000, play() {} },
    checks: {
      cycle: 6500,
      play(card, timers) {
        const count = card.querySelector(".count");
        count.textContent = "0";
        timers.push(setTimeout(() => countUp(count, 600), 4 * 120 + 300));
      },
    },
    run: {
      cycle: 8000,
      play(card, timers) {
        typeOut(card.querySelector(".typed"), timers);
      },
    },
    import: {
      cycle: 8000,
      play(card, timers) {
        const typed = typeOut(card.querySelector(".typed"), timers, 28);
        const count = card.querySelector(".count");
        count.textContent = "0";
        timers.push(setTimeout(() => countUp(count, 500), typed + 16 * 45 + 200));
      },
    },
    protocols: {
      cycle: 15000,
      play(card, timers) {
        const tabs = [...card.querySelectorAll(".seg")];
        const panels = [...card.querySelectorAll(".panel")];
        const show = (n) => {
          tabs.forEach((t, i) => t.classList.toggle("on", i === n));
          panels.forEach((p, i) => p.classList.toggle("on", i === n));
        };
        show(0);
        tabs.forEach((_, i) => timers.push(setTimeout(() => show(i), i * 3000)));
      },
    },
  };

  if (!reduce && "IntersectionObserver" in window) {
    for (const card of document.querySelectorAll("[data-demo]")) {
      const demo = demos[card.dataset.demo];
      if (!demo) continue;
      card.classList.add("armed");
      let timers = [];
      let loop = null;
      const run = () => {
        timers.forEach(clearTimeout);
        timers = [];
        card.classList.remove("on");
        // One frame with the start state, so the transitions run again.
        requestAnimationFrame(() =>
          requestAnimationFrame(() => {
            card.classList.add("on");
            demo.play(card, timers);
          }),
        );
      };
      const stop = () => {
        clearInterval(loop);
        loop = null;
        timers.forEach(clearTimeout);
        timers = [];
      };
      new IntersectionObserver(
        ([entry]) => {
          if (entry.isIntersecting && !loop) {
            run();
            loop = setInterval(run, demo.cycle);
          } else if (!entry.isIntersecting && loop) {
            stop();
          }
        },
        { threshold: 0.4 },
      ).observe(card);
    }
  }

  // The measured facts count up once, when they come into view.
  const facts = [...document.querySelectorAll(".facts .count")];
  if (facts.length && !reduce && "IntersectionObserver" in window) {
    const seen = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          countUp(entry.target, 1200);
          seen.unobserve(entry.target);
        }
      },
      { threshold: 0.6 },
    );
    facts.forEach((el) => seen.observe(el));
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
