---
name: Getman
description: A local-first API client in Material Design 3, at compact density, seeded from the logo's indigo.
colors:
  seed: "#3F51B5"
  primary: "#4355B9"
  on-primary: "#FFFFFF"
  primary-container: "#DEE0FF"
  on-primary-container: "#293CA0"
  secondary-container: "#E0E1F9"
  on-secondary-container: "#434659"
  tertiary: "#72508F"
  tertiary-container: "#F1DAFF"
  error: "#BA1A1A"
  error-container: "#FFDAD6"
  surface: "#FBF8FE"
  surface-container-lowest: "#FFFFFF"
  surface-container-low: "#F5F2F9"
  surface-container: "#EFEDF3"
  surface-container-high: "#EAE7ED"
  surface-container-highest: "#E4E1E7"
  on-surface: "#1B1B20"
  on-surface-variant: "#454650"
  outline: "#767681"
  outline-variant: "#C6C5D1"
  inverse-surface: "#303035"
  ok: "#1A6C31"
  info: "#006496"
  warn: "#865300"
  violet: "#7848A2"
  dark-primary: "#BAC3FF"
  dark-on-primary: "#08218A"
  dark-surface: "#131317"
  dark-surface-container: "#1F1F24"
  dark-on-surface: "#E4E1E7"
  dark-outline-variant: "#454650"
typography:
  display:
    fontFamily: "Google Sans Flex, system-ui, sans-serif"
    fontSize: "57px"
    fontWeight: 400
    lineHeight: 1.1
  headline-large:
    fontFamily: "Google Sans Flex, system-ui, sans-serif"
    fontSize: "36px"
    fontWeight: 500
    lineHeight: 1
  headline:
    fontFamily: "Google Sans Flex, system-ui, sans-serif"
    fontSize: "28px"
    fontWeight: 400
    lineHeight: 1.25
  title-large:
    fontFamily: "Google Sans Flex, system-ui, sans-serif"
    fontSize: "22px"
    fontWeight: 400
    lineHeight: 1.3
  title:
    fontFamily: "Google Sans Flex, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 500
    lineHeight: 1.4
  body:
    fontFamily: "Google Sans Flex, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.45
  label:
    fontFamily: "Google Sans Flex, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: 1.25
    letterSpacing: "0.01em"
  meta:
    fontFamily: "Google Sans Flex, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.45
  code:
    fontFamily: "Google Sans Code, ui-monospace, monospace"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.45
  method:
    fontFamily: "Google Sans Code, ui-monospace, monospace"
    fontSize: "11px"
    fontWeight: 700
    letterSpacing: "0.02em"
rounded:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "28px"
  full: "999px"
spacing:
  s1: "4px"
  s2: "8px"
  s3: "12px"
  s4: "16px"
  s5: "24px"
  s6: "32px"
  control: "30px"
  bar: "44px"
components:
  button:
    backgroundColor: "transparent"
    textColor: "{colors.primary}"
    typography: "{typography.label}"
    rounded: "{rounded.full}"
    padding: "0 16px"
    height: "30px"
  button-filled:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    typography: "{typography.label}"
    rounded: "{rounded.full}"
    padding: "0 16px"
    height: "30px"
  button-text:
    backgroundColor: "transparent"
    textColor: "{colors.primary}"
    typography: "{typography.label}"
    rounded: "{rounded.full}"
    padding: "0 12px"
    height: "30px"
  text-field:
    backgroundColor: "{colors.surface-container-lowest}"
    textColor: "{colors.on-surface}"
    typography: "{typography.body}"
    rounded: "{rounded.xs}"
    padding: "0 10px"
    height: "30px"
  tab:
    backgroundColor: "transparent"
    textColor: "{colors.on-surface-variant}"
    typography: "{typography.label}"
    padding: "0 12px"
    height: "36px"
  tab-selected:
    textColor: "{colors.on-surface}"
  list-item-selected:
    backgroundColor: "{colors.secondary-container}"
    textColor: "{colors.on-secondary-container}"
  menu:
    backgroundColor: "{colors.surface-container}"
    textColor: "{colors.on-surface}"
    typography: "{typography.body}"
    rounded: "{rounded.xs}"
    padding: "4px 0"
  menu-item:
    height: "32px"
    padding: "0 12px"
  dialog:
    backgroundColor: "{colors.surface-container-high}"
    textColor: "{colors.on-surface}"
    rounded: "{rounded.lg}"
  snackbar:
    backgroundColor: "{colors.inverse-surface}"
    textColor: "#F2F0F6"
    rounded: "{rounded.xs}"
    padding: "8px 16px"
  segmented-button:
    textColor: "{colors.on-surface}"
    rounded: "{rounded.full}"
    height: "28px"
  segmented-button-selected:
    backgroundColor: "{colors.secondary-container}"
    textColor: "{colors.on-secondary-container}"
  chip:
    textColor: "{colors.on-surface-variant}"
    rounded: "{rounded.sm}"
  kbd:
    backgroundColor: "{colors.surface-container-low}"
    textColor: "{colors.on-surface-variant}"
    typography: "{typography.code}"
    rounded: "{rounded.xs}"
    padding: "2px 5px"
---

# Design System: Getman

## Overview

**Creative North Star: "A calm instrument"**

Getman is a developer tool people keep open all day beside an editor and a terminal. It follows Material Design 3 at compact density: one color scheme generated from the logo's indigo, tonal surfaces instead of lines and shadows wherever it can, sentence-case labels, rounded controls, and state layers for hover and press. The look is quiet and exact so the request and the response carry the screen.

Color carries meaning. The primary indigo marks the one main action of a view (Send, Invoke, Run) and focus; fixed status hues mark methods and response classes; everything else is neutral surface and text. Light and dark come from the same scheme, follow the OS, and can be overridden with the theme toggle.

**Key Characteristics:**
- One seed (#3F51B5), Material Color Utilities' Tonal Spot tones with the seed's chroma kept in the primary palette.
- Google Sans Flex for interface text, Google Sans Code for code, URLs, keys and values, both self-hosted.
- Compact density: 30px controls, 36px tabs, 28 to 32px rows, 13px body text.
- Shapes from Material's scale: 4px fields and menus, 8px chips and the request line, 16px dialogs, full rounding on buttons and segmented buttons.
- Hover and press are state layers (8% and 10% washes of the content color), never inversions.
- Material Symbols Rounded for every icon.

## Colors

The roles are generated, not picked: `web/src/styles/tokens.css` holds every role for light and dark, made with Material Color Utilities 0.4.0 from the seed. Do not edit it by hand; regenerate it.

### Primary
- **Primary** (primary): the filled main action of each view, focus rings and the focused field's outline, the selected tab's indicator, links and text buttons, the text caret, the logo mark in the top bar.
- **Primary container** (primary-container / on-primary-container): text selection and highlighted search matches.

### Secondary
- **Secondary container** (secondary-container / on-secondary-container): selection in lists and toggles: the current request in the tree, the highlighted palette row, the on segment of a segmented button, the selected environment, filter chips that are on.

### Tertiary
- **Tertiary** (tertiary on tertiary-container): `{{variables}}` in the URL bar and editors.

### Status and methods
Fixed hues at Material's custom-color tones (40 on light, 80 on dark, with 90 and 30 containers), so meaning never shifts with the seed.
- **Ok** (ok): 2xx, passing tests, GET.
- **Info** (info): 3xx, PUT, sent messages.
- **Warn** (warn): POST, warnings.
- **Violet** (violet): PATCH.
- **Error** (error / error-container): 4xx and 5xx, DELETE, failing counts, missing variables, error notices, destructive actions.

### Neutral
- **Surface** (surface): the page.
- **Surface containers** (lowest to highest): fields and editors sit on the lowest; menus on the container; dialogs on high; tab strips and quiet fills step between them.
- **On surface / on surface variant** (on-surface, on-surface-variant): text and secondary text. **Outline** (outline) draws text-field borders and quiet captions; **outline variant** (outline-variant) draws dividers.
- **Inverse surface**: snackbars (notices).

### Named Rules
**The Generated Scheme Rule.** Every color is a role from `tokens.css`. Older names (`--ink`, `--paper`, `--rule`, `--accent` and the `--t0` to `--t10` ramp) are aliases of roles, kept so older components still read; new code uses the role names.

**The Meaning-Only Color Rule.** Hue appears only where it encodes something: the main action, selection, a method, a status, a variable, a failure. Decorative color is not used.

**The One Primary Rule.** Each view offers one filled primary button.

## Typography

**Interface Font:** Google Sans Flex (self-hosted woff2, SIL OFL; Latin, weights 400 to 700, optical size fixed at 16)
**Code Font:** Google Sans Code (self-hosted woff2, SIL OFL; Latin, weights 400 to 700)

### Hierarchy
- **Display** (57px, 400): reserved; the status code uses Headline Large.
- **Headline Large** (36px, 500, tabular figures): the response status code.
- **Headline** (28px, 400): empty-state headings and folder names.
- **Title Large** (22px, 400): the workspace name and other view titles.
- **Title** (16px, 500): the editable request name and the response reason phrase.
- **Body** (13px, 400, line-height 1.45): all working text, inputs, menus and tree rows; paragraphs cap at 52ch.
- **Label** (12px, 500, letter-spacing 0.01em): section heads, column heads, counts and captions; set with the `.bitmap` class, a name older than this design. Tabs and buttons use 13px at 500.
- **Meta** (12px, 400): paths and secondary lines in lists.
- **Code** (13px; 14px in the URL line): editors, URLs, keys and values.
- **Method** (Google Sans Code 11px, 700, uppercase): method marks, in the method's color.

### Named Rules
**The Sentence Case Rule.** Labels, tabs and buttons are set in sentence case as written. Only method marks are uppercase.

## Layout

A fixed application grid: a 44px top bar, a resizable sidebar (default 280px), and a 24px status bar. The main column stacks the open-request tab strip, the request title row, the request line (40px), then a split of request editors beside the response, resizable and switchable to stacked rows. Below 900px the panes stack; in narrow layouts the sidebar becomes a fixed overlay over a scrim, and the top bar hides the palette text and wordmark below 720px.

Spacing runs on a 4px base: 4, 8, 12, 16, 24, 32px (`--s1` to `--s6`), with 30px controls. Content starts 16px from the edges; inner gaps are 8 or 12px.

## Elevation & Depth

In-flow surfaces are flat and separated by tonal steps of the surface containers and by outline-variant dividers. Only what floats above the page has a shadow, at Material's levels:
- **Level 1** (`--elevation-1`): a filled button on hover.
- **Level 2** (`--elevation-2`): menus, autocomplete lists, the timing panel.
- **Level 3** (`--elevation-3`): dialogs and snackbars.

Dialogs open over a 32% scrim.

## Shapes

Material's shape scale (`--r-xs` to `--r-full`): text fields, menus, snackbars and keycaps at 4px; chips, the request line field and badges at 8px; dialogs at 16px; buttons, icon buttons and segmented buttons fully rounded. Rows in lists stay square so they run edge to edge. The selected tab's indicator is a 3px bar with rounded top corners.

## Components

### Buttons
- **Outlined** (`.btn`): 30px, full rounding, outline-variant border, primary label, 13px weight 500.
- **Filled** (`.btn.primary`): primary fill, on-primary label; level 1 shadow on hover. One per view.
- **Text** (`.btn.quiet`): no border, primary label.
- **Danger** (`.btn.danger`): error label.
- **Icon button** (`.icon-btn`): 28px circle, on-surface-variant icon.
- All show hover and press as a state layer of the label color; disabled at 38% opacity.

### Text fields
- **Style:** 30px, 4px corners, 1px outline border on the lowest surface container, 13px text (code face for `.mono`).
- **Hover / Focus:** the border steps to on-surface on hover and to a 2px primary outline on focus.
- **Request line:** method and URL share one outlined field with 8px corners (method in the method's color, URL in 14px code with variables in tertiary); Send, Invoke or Connect is a filled button beside it that shows a circular progress indicator while the request is out.

### Checkboxes
Material's checkbox at 16px: a 2px on-surface-variant outline that fills with primary and shows a white check.

### Tabs
Material's secondary tabs: 13px labels in on-surface-variant on a strip with an outline-variant divider; the selected one turns on-surface with a 3px primary indicator. Counts follow in tabular figures, failing counts in error.

### Segmented buttons
Body type and response view modes: fully rounded groups with outline dividers; the on segment fills with secondary container.

### Open-request tabs
A strip on the surface container under the top bar. Tabs have 8px top corners; the active one takes the page's surface and weight 500, so it joins the request below, and the others show a state layer on hover with a short outline-variant divider between them. Unsaved titles set italic; a dirty tab's close mark is primary. A dragged tab lands at a 2px primary line.

### Status bar
24px, on the surface with an outline-variant rule above: version, workspace path and request count in on-surface-variant, separated by 3px outline dots; Console and Shortcuts are quiet links with a state layer, and Console turns primary while the console is open.

### Navigation (sidebar tree)
28px rows with a twisty, the method mark in its color, and the name. Hover fills the surface container; the current request fills secondary container. Picked rows show a 3px primary bar; drop targets show the primary container.

### Menus and dialogs
- **Menu:** surface container, 4px corners, level 2 shadow, 32px items with an 18px icon column; hover is a state layer, danger items in error.
- **Dialog:** surface container high, 16px corners, level 3 shadow, a 32% scrim; footer actions right-aligned. Opens with a short rise on the emphasized decelerate curve.

### Notices
Snackbars at the bottom right: inverse surface, 4px corners, level 3 shadow, an 8px status dot and a text action in inverse primary. Errors use the error container.

### Response header
The head of every response: a faint tint of the status color over the surface, the status code at 36px in that color with the reason phrase beside it, then Time, First byte, Size, Protocol and Tests as label and value pairs. Time opens the timing panel. While a request is out, an indeterminate linear progress bar runs along the top. WebSocket, Server-Sent Events and Socket.IO connections use the same header: the state (Open, Closed) in its color on a faint tint, the handshake facts as pairs, and the linear progress bar while connecting.

### Runner
The run's results grid has one 16px cell per request and iteration with 4px corners: the highest surface container until it runs, then ok, error or outline for passed, failed or skipped; the chosen cell gets a 2px primary ring. Errors in the runner, a response or a connection sit on the error container.

### Orbiting mark
The decoration of the empty workspace and the 401 page: the logo mark on a primary-container disc, with Material Symbols for what Getman does on an inner ring and method marks on an outer one turning the other way (`Orbit.svelte`, a port of Magic UI's Orbiting Circles). Rings are outline-variant hairlines, turns take 36 and 54 seconds, and reduced motion stops them. It shows beside the empty workspace's text only when the main area is at least 880px wide. While the workspace loads, the mark alone breathes in primary.

### Keys
`kbd` is an 11px code-face keycap: lowest-container fill, outline-variant border with a 2px bottom edge, 4px corners.

### Motion
Material's easing and durations: standard `cubic-bezier(0.2, 0, 0, 1)`, emphasized decelerate `cubic-bezier(0.05, 0.7, 0.1, 1)` for things arriving, 150ms for state changes, 250ms for dialogs and snackbars. Reduced motion collapses every animation and transition.

## Website

The documentation site (`site/`) uses the same system: the build puts `tokens.css` before `site/src/site.css`, the fonts and Material Symbols are the app's, and every page reads in light and dark. Reading sizes are larger than the app's: 17px body text, 20px leads, headings in Google Sans Flex at 500.

- **Top bar:** the logo and wordmark, text links with state layers (the current section in secondary container), Material's search bar on the high container, and the theme toggle.
- **Landing page:** a hero with the orbiting mark, the download for the visitor's system and the install command in an outlined field; the real screenshot in a 16px frame with the level 3 shadow; a bento grid of feature cards (28px corners on the low container) whose mockups replay what the app and the command line really show, each looping while on screen and still under reduced motion; measured facts; a closing download on the primary container.
- **Docs pages:** the guide on an outlined card with 16px corners and the level 1 shadow, over a tinted page (light: the low container under the lowest; dark: the lowest container under the surface), with the sidebar and the on-this-page list on the tint beside it. The sidebar's current page is a secondary-container pill; the prose is capped at 46rem in the middle of the card, with a small outlined Copy page button under the lead; code blocks and tables sit on the low container with 12px corners; the on-this-page list has a primary marker; previous and next are outlined cards.
- **Search:** a dialog on the high container with 28px corners over a 32% scrim; the selected result is a secondary-container pill.

## Do's and Don'ts

### Do:
- **Do** take every color from a role in `tokens.css`, and regenerate the file to change the scheme.
- **Do** show hover and press as state layers (`--hover`, `--press`) of the content color.
- **Do** use the secondary container for selection and the primary for the one main action.
- **Do** keep labels in sentence case and use Google Sans Code for anything a person types or copies.
- **Do** use Material Symbols Rounded from `icons.ts`, at 18px (16px inside rows and tabs).
- **Do** keep focus visible as a 2px primary outline.

### Don't:
- **Don't** invert a control to solid ink on hover or selection.
- **Don't** put more than one filled primary button in a view, or use color for decoration.
- **Don't** add shadows to in-flow surfaces; only menus, dialogs and snackbars float.
- **Don't** load fonts, icons or images from the network; everything is self-hosted.
- **Don't** set text in uppercase except method marks.
