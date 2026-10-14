---
name: Getman
description: A local-first API client set like an Emigre bitmap type specimen.
colors:
  newsprint-paper: "#f2f0eb"
  field-white: "#fbfaf7"
  sunken-paper: "#e8e5de"
  rich-ink: "#0a0a0a"
  ink-secondary: "#4f4d48"
  ink-tertiary: "#6b6862"
  rule-hairline: "#c9c5bb"
  rule-faint: "#dcd8cf"
  ramp-mid: "#aeaaa0"
  signal-red: "#e5011a"
  signal-red-pressed: "#c00016"
  signal-red-text: "#c4001a"
  signal-red-wash: "#fbe3e5"
  accent-ink: "#ffffff"
  status-ok: "#0e7a3d"
  status-redirect: "#1e4fd0"
  status-error: "#c4001a"
  status-warn: "#a35a00"
  method-patch: "#7b3fb3"
  variable-ink: "#7b3fb3"
  variable-wash: "#efe6f8"
  carbon-paper: "#0e0e0d"
  carbon-field: "#141412"
  carbon-rule: "#8a877f"
  carbon-signal-red: "#ff2a3d"
  carbon-accent-ink: "#0e0e0d"
typography:
  display:
    fontFamily: "Departure Mono, ui-monospace, monospace"
    fontSize: "88px"
    fontWeight: 400
    lineHeight: 0.82
    letterSpacing: "0"
  headline:
    fontFamily: "Departure Mono, ui-monospace, monospace"
    fontSize: "33px"
    fontWeight: 400
    lineHeight: 1
    letterSpacing: "0"
  title-bitmap:
    fontFamily: "Departure Mono, ui-monospace, monospace"
    fontSize: "22px"
    fontWeight: 400
    lineHeight: 1.1
    letterSpacing: "0"
  label:
    fontFamily: "Departure Mono, ui-monospace, monospace"
    fontSize: "11px"
    fontWeight: 400
    lineHeight: 1
    letterSpacing: "0"
  title:
    fontFamily: "system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, Noto Sans, sans-serif"
    fontSize: "15px"
    fontWeight: 650
    lineHeight: 1.45
  body:
    fontFamily: "system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, Noto Sans, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.45
  meta:
    fontFamily: "system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, Noto Sans, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.45
  code:
    fontFamily: "ui-monospace, Cascadia Mono, SF Mono, Menlo, Consolas, Liberation Mono, monospace"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.45
rounded:
  none: "0px"
spacing:
  s1: "4px"
  s2: "8px"
  s3: "12px"
  s4: "16px"
  s5: "24px"
  s6: "32px"
  control: "28px"
  bar: "40px"
components:
  button:
    backgroundColor: "{colors.field-white}"
    textColor: "{colors.rich-ink}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "0 12px"
    height: "28px"
  button-hover:
    backgroundColor: "{colors.rich-ink}"
    textColor: "{colors.newsprint-paper}"
  button-primary:
    backgroundColor: "{colors.signal-red}"
    textColor: "{colors.accent-ink}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "0 13px"
    height: "28px"
  button-primary-hover:
    backgroundColor: "{colors.signal-red-pressed}"
    textColor: "{colors.accent-ink}"
  button-quiet:
    backgroundColor: "transparent"
    textColor: "{colors.rich-ink}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "0 12px"
    height: "28px"
  button-quiet-hover:
    backgroundColor: "{colors.sunken-paper}"
  input:
    backgroundColor: "{colors.field-white}"
    textColor: "{colors.rich-ink}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "0 8px"
    height: "28px"
  tab:
    backgroundColor: "transparent"
    textColor: "{colors.ink-tertiary}"
    typography: "{typography.label}"
    padding: "0 10px"
    height: "32px"
  tab-selected:
    textColor: "{colors.rich-ink}"
  menu-item:
    backgroundColor: "{colors.field-white}"
    textColor: "{colors.rich-ink}"
    typography: "{typography.body}"
    padding: "0 12px"
    height: "28px"
  menu-item-hover:
    backgroundColor: "{colors.rich-ink}"
    textColor: "{colors.newsprint-paper}"
  kbd:
    backgroundColor: "{colors.field-white}"
    textColor: "{colors.ink-secondary}"
    typography: "{typography.label}"
    padding: "2px 4px"
  notice:
    backgroundColor: "{colors.rich-ink}"
    textColor: "{colors.newsprint-paper}"
    typography: "{typography.body}"
    padding: "8px 12px"
---

# Design System: Getman

## Overview

**Creative North Star: "The Bitmap Specimen Sheet"**

Getman is an API client set like an Emigre bitmap type specimen. Crisp, smooth working text carries everything people read and type; one coarse bitmap face, Departure Mono, appears only at whole multiples of its 11px pixel grid, and only for what deserves a glance: section heads and controls in small caps, the workspace name, counts, and the response status set as an 88px numeral. The page is newsprint and rich ink, ruled with black lines, with a single synthetic signal red for the primary action, live selection markers and destructive intent.

The shell is dense and flat: square corners everywhere, 1px black structural rules, a 2px ink frame around the one line that matters (method, URL, Send), hover states that invert to solid ink. Depth comes from print materials rather than light: halftone dot fields, dot-matrix scans, a dotted focus outline. The signature is the response specimen, where the status numeral sits on a halftone tile whose dots swell with response time and take the status colour. Motion is pixel-stepped (`steps()`), never eased into smoothness, and collapses under reduced motion.

Light is newsprint (#f2f0eb) with ink (#0a0a0a); dark is carbon (#0e0e0d) with paper-coloured text and a brighter red, chosen from the OS and overridable with a toggle. It refuses the category default of a charcoal three-pane with a lone orange Send button.

**Key Characteristics:**
- Two type systems with a hard border: bitmap face at 11/22/33/44/88px only, smooth system faces for working text and code.
- Paper-to-ink neutral ramp (11 steps) is the only source of neutrals.
- One signal red; colour otherwise carries meaning only (methods, status, variables, failures).
- Zero radius; stepped 4px pixel corners on primary controls only.
- Hover inverts to solid ink; focus is a 2px dotted ink outline.
- Halftone and dot-matrix fields are real surfaces, not decoration.
- Pixel-drawn SVG icons on a 7x7 grid at 7, 14 or 21px.

## Colors

Newsprint and rich ink with one synthetic red and a small set of functional status inks; every neutral steps along a single paper-to-ink ramp.

### Primary
- **Signal Red** (signal-red): the Send button and other primary actions, the 3px marker on the active request tab, the 11px square of the wordmark, drop targets in the tree, the text caret and the editor cursor. Pressed/hover deepens to Signal Red Pressed. On paper, red used as text uses the darker Signal Red Text for contrast; Signal Red Wash fills a drop-into-folder target.

### Tertiary (functional inks)
- **Status Green** (status-ok): 2xx status numeral and halftone tone, passing test counts, GET method marks, JSON strings in the editor.
- **Redirect Blue** (status-redirect): 3xx status tone, PUT marks, numbers in the editor.
- **Error Red** (status-error): 4xx/5xx tone, DELETE marks, failing counts, broken tree items, missing variables, error notices.
- **Warn Amber** (status-warn): POST marks, editor function names, warnings.
- **Variable Violet** (variable-ink on variable-wash): `{{variables}}` highlighted in the URL and editors; also PATCH marks (method-patch).

### Neutral
- **Newsprint Paper** (newsprint-paper): the page and every panel background.
- **Field White** (field-white): inputs, editors, menus, secondary buttons; a half-step lighter than paper so fields read as writable.
- **Sunken Paper** (sunken-paper): the request tab strip, quiet hover fills, list hover rows.
- **Rich Ink** (rich-ink): text, structural rules, hover inversion fill, selected tree row, notices, text selection.
- **Ink Secondary / Ink Tertiary** (ink-secondary, ink-tertiary): supporting text, placeholders, inactive tabs, counts.
- **Hairline / Faint Rule** (rule-hairline, rule-faint): input borders, inner dividers, column separators.
- **Ramp Mid** (ramp-mid): scrollbar thumbs, the soft halftone, the idle numeral.
- **Carbon** (carbon-paper, carbon-field, carbon-rule): the dark theme inverts the ramp; structural rules soften to a warm grey so the black-rule grammar survives on carbon. Primary text on Carbon Signal Red is carbon (carbon-accent-ink), not white.

### Named Rules
**The One Ramp Rule.** Every neutral is a step of the paper-to-ink ramp (`--t0` to `--t10`); never introduce a grey from outside it. The dark theme reverses the ramp rather than inventing new neutrals.

**The Meaning-Only Colour Rule.** Hue appears only where it encodes something: method, status, variable, failure, or the one primary action. Decorative colour is not used.

**The One Red Rule.** Signal red marks the primary action and live selection. Each view offers one red primary action.

## Typography

**Display Font:** Departure Mono (self-hosted woff2, SIL OFL), falling back to ui-monospace
**Body Font:** system-ui stack (Segoe UI, Roboto, Helvetica Neue, Arial, Noto Sans)
**Label/Mono Font:** Departure Mono for labels; ui-monospace stack (Cascadia Mono, SF Mono, Menlo, Consolas) for code, URLs and values

**Character:** A coarse bitmap face used like specimen captions and display sizes, against invisible, crisp system text for everything read at length. The contrast between the two is the identity.

### Hierarchy
- **Display** (Departure Mono 400, 88px, line-height 0.82): the response status numeral only. Drops to 44px under 520px.
- **Headline** (Departure Mono 400, 33px, line-height 1, uppercase): empty-state headings. 44px is reserved for full-screen states.
- **Title Bitmap** (Departure Mono 400, 22px, uppercase): the wordmark, the workspace name, the response reason phrase.
- **Label** (Departure Mono 400, 11px, line-height 1, uppercase, no smoothing): buttons, tabs, method marks, section heads ("Query parameters"), column heads, specimen size labels, kbd keys, footer and status bar.
- **Title** (system 650, 15px): the editable request name.
- **Body** (system 400, 13px, line-height 1.45): all working text, inputs, menus, tree rows; paragraphs cap at 52ch.
- **Meta** (system 400, 12px): paths and secondary lines in lists.
- **Code** (ui-monospace 13px; 14px in the URL line): editors, URLs, keys and values.

### Named Rules
**The Whole-Pixel Rule.** Departure Mono is set only at 11, 22, 33, 44 or 88px (`--b1` to `--b8`), uppercase, weight 400, letter-spacing 0, with font smoothing off. No other size, weight or tracking.

**The Two-Faces Rule.** Anything a person reads in sentences or types goes in the smooth faces. The bitmap face is for glances: labels, counts, status, names.

## Layout

A fixed application grid: a 40px top bar, a resizable sidebar (default 280px) separated by a 1px rule, and a 24px status bar. The main column stacks the open-request tab strip (34px), the request title row, the request line (40px, 2px ink frame), then a split of request editors beside the response, resizable and switchable to stacked rows. Below 900px the panes stack; in narrow layouts the sidebar becomes a fixed overlay (min(320px, 86vw)) over a 30% ink scrim, and the top bar hides the palette text and wordmark below 720px.

Spacing runs on a 4px base: 4, 8, 12, 16, 24, 32px (`--s1` to `--s6`), with 28px controls and 40px bars. Density is high: tables are grid rows with 28px cells, list rows are 28px, tabs 32px. Horizontal edges of content start at 16px; inner gaps are 8 or 12px.

## Elevation & Depth

The system is flat. Panels are separated by rules and tonal steps of the ramp (paper, field, sunken), never by shadows. Depth is a print effect: halftone dot fields (radial-gradient dots on a 4px grid) behind the specimen and the modal backdrop, and ink inversion for the hovered or selected thing. Only surfaces that float above the shell carry a soft ambient shadow, and only so they detach from the page.

### Shadow Vocabulary
- **Dialog lift** (`box-shadow: 0 24px 60px -24px color-mix(in srgb, var(--t10) 55%, transparent)`): modal dialogs, together with a 2px ink border and a halftone backdrop.
- **Menu lift** (`box-shadow: 0 6px 18px -6px color-mix(in srgb, var(--t10) 35%, transparent)`): context and dropdown menus.
- **Pressed double frame** (`box-shadow: inset 0 0 0 2px var(--fill), inset 0 0 0 3px var(--accent-ink)`): a primary button while pressed.

### Named Rules
**The Ink Not Light Rule.** In-flow surfaces never cast shadows. Hover and selection invert to solid ink; depth comes from halftone, rules and the paper ramp.

## Shapes

Square everything: 0px radius on every control, panel, dialog, tab and badge. Structure is drawn with 1px black rules (`--rule`), inner divisions with hairline rules, and emphasis with heavier ink: the 2px frame around the request line and dialogs, 3px bars for the active tab (red on request tabs, ink under pane tabs). Primary controls get stepped pixel corners: a 4px three-step stair painted in the colour behind the button (`--corner`), so the shape reads as a bitmap rectangle. Checkboxes are 14px squares filled with a 6px ink square; select chevrons and icons are built from pixels.

## Components

### Buttons
Bitmap-labelled, square, and inverting.
- **Shape:** square (0px), 28px tall, 12px side padding, 11px Departure Mono uppercase.
- **Default:** field-white fill with a 1px ink rule. Hover inverts to ink fill with paper text; active nudges down 1px; disabled at 45% opacity.
- **Primary:** signal red fill, white text (carbon text in dark), no border, stepped 4px pixel corners in the surrounding colour. Hover deepens the red; pressing draws the specimen's double frame inside it. In the request line, Send is 104px wide and full height, ruled from the URL by 2px ink; while sending it shows a stepped dot-matrix scan.
- **Quiet:** transparent with no border; hover fills sunken paper.
- **Danger:** default at rest; hover fills error red with paper text.
- **Icon button:** 26px square, transparent, ink-secondary icon; hover inverts to ink.

### Inputs / Fields
- **Style:** 28px, field-white, 1px hairline rule, square, 13px system text (code face for `.mono` fields); placeholders in ink-tertiary.
- **Hover / Focus:** border steps to ramp mid-dark on hover and to full ink on focus. Caret is signal red.
- **Selects:** native appearance removed; a pixel chevron drawn from two gradients.
- **Request line:** method select (bitmap, method colour) | URL editor (14px code, variables in violet wash) | Send, inside a 40px 2px-ink frame.

### Tabs
- **Pane tabs:** 11px bitmap labels in ink-tertiary over a hairline-ruled strip; selected goes ink with a 3px ink bar inset 8px from each side; counts follow in tertiary, failing counts in error red.
- **Request tabs:** a sunken strip of tabs divided by hairlines; the active tab lifts to paper with a 3px signal red bar on top and an open bottom edge. Unsaved titles set italic; the close mark turns red when dirty. Overflowing rows fade at the cut edge (40px mask).

### Navigation (sidebar tree)
- 28px rows: twisty, right-aligned 11px method mark in its method colour, label. Hover fills sunken paper; the current request inverts to ink with paper text (the method mark turns paper too). Folders set in 600 weight. Drop-into shows red wash with a 1px red inset; drop-between shows a 2px red line.
- The workspace name heads the sidebar at 22px bitmap; a Requests / History tab switch sits under the filter field.

### Menus and Dialogs
- **Menu:** field-white, 1px ink rule, menu lift; 28px items with a 14px icon column. Hover/focus inverts to ink; danger items in error red, inverting to red.
- **Dialog:** paper, 2px ink border, dialog lift, halftone backdrop; footer actions right-aligned over a hairline rule. Opens with a 3-step 160ms rise.

### Notices
Solid ink bars (error notices solid error red) bottom right, with a 6px square status dot; arrive in 3 steps over 180ms.

### Keys
`kbd` is an 11px bitmap keycap: field-white, hairline border with a 2px bottom edge.

### Response Specimen (signature)
The head of every response. An SVG halftone tile of dots on a 6px grid fills the block; dot radius grows with response time on a log scale from 1ms (pale screen) to 10s (near solid), densest under the numeral and thinning to the right. Dots take the status tone (green 2xx, blue 3xx, red 4xx/5xx, ink-tertiary idle or sending) at `--dot-strength` (46% light, 30% dark). The status code sits on it as the 88px numeral in the same tone; a network failure sets "ERR" reversed out of a red block. The reason phrase follows at 22px on a paper chip; Time, First byte, Size, Protocol and Tests sit beneath as specimen size labels (11px bitmap caption over 650-weight tabular figures) on paper chips divided by hairlines. Sending pulses the dots in a 4-step scan and types out a dot row in place of the numeral; idle shows "---" and the send shortcut.

### Motion
All motion is stepped: dialogs and notices arrive in 3 steps, scans and type-ons run in 4 steps, the cursor blinks in 2. Reduced motion collapses every animation and transition to 0.01ms.

## Do's and Don'ts

### Do:
- **Do** set Departure Mono only at 11, 22, 33, 44 or 88px, uppercase, weight 400, smoothing off.
- **Do** take every neutral from the paper-to-ink ramp and every space from the 4px scale (4, 8, 12, 16, 24, 32px).
- **Do** draw structure with 1px ink rules and emphasis with 2px ink frames or 3px active bars.
- **Do** invert to solid ink on hover for buttons, menu items and the current list row.
- **Do** give primary controls the stepped 4px pixel corner, painting `--corner` with the colour behind them.
- **Do** draw icons as pixel SVG on the 7x7 grid with `shape-rendering: crispEdges`, at 7, 14 or 21px.
- **Do** animate with `steps()` and let reduced motion collapse it.
- **Do** keep focus visible as a 2px dotted ink outline.

### Don't:
- **Don't** round corners; the radius is 0px everywhere.
- **Don't** put more than one signal-red control in one view, or use red for decoration.
- **Don't** set sentences, inputs or code in the bitmap face, or the bitmap face at fractional sizes.
- **Don't** add shadows to in-flow surfaces; only dialogs and menus float.
- **Don't** use smooth eased motion for reveals or ambient animation.
- **Don't** load fonts, icons or images from the network; everything is self-hosted.
- **Don't** fall back to the category's charcoal three-pane with an orange Send button.
