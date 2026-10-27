# Browser UI

`getman` opens the workspace in your browser, on 127.0.0.1. This page
covers the parts of the UI that save you work day to day. Press
<kbd>?</kbd> in the UI for every shortcut.

## Tabs

Open tabs come back when you reload the page or restart `getman`, each
workspace with its own. So do changes you have not saved, and requests
you never saved at all. If a file was renamed or moved while the UI was
closed, Getman finds it by its name in the same folder or its file name
elsewhere; if a file with unsaved changes is gone, the changes open as a
new request rather than disappear.

The tabs live in this browser's storage. A different browser, or the UI
on another port, starts with none.

| Keys | Does |
| --- | --- |
| <kbd>Alt</kbd> <kbd>Shift</kbd> <kbd>T</kbd> | Reopen the tab closed last, with its unsaved changes |
| <kbd>Shift</kbd> <kbd>←</kbd> <kbd>→</kbd> | On a focused tab: move it |
| <kbd>Del</kbd> | On a focused tab: close it |

Browsers keep <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>T</kbd> for their own
tabs; Getman answers it too where the browser lets it through. Drag a tab
to move it. Right-click one, or press the menu key on it, to close
others, close those to the right, close all, duplicate a request, reveal
the item in the sidebar or copy its path.

## Sidebar

- **Collapse all** folds every folder.
- **Sort A to Z** shows each folder's items by name, folders first. It
  changes only the view in this browser; the files and their order stay
  as they are, and dragging to reorder is off while it is on.
- <kbd>Ctrl</kbd> click (<kbd>Cmd</kbd> on a Mac) picks several items,
  <kbd>Shift</kbd> click or <kbd>Shift</kbd> and the arrow keys a range,
  <kbd>Ctrl</kbd> <kbd>Space</kbd> the focused one, and <kbd>Esc</kbd>
  clears them. The menu of a picked item then duplicates, moves or
  deletes them all; <kbd>Del</kbd> deletes them and dragging one moves
  them all.

## The trash

Deleting a request or folder moves it to `.getman/trash/` in the
workspace. A notice offers **Undo**, and so does <kbd>Ctrl</kbd>
<kbd>Z</kbd> outside a text field. The **Trash** tab of the sidebar lists
what was deleted and puts an item back where it was; if something new
has taken its name, the item comes back with a number. **Empty trash**
deletes everything in it for good, after asking.

`.getman/` has its own `.gitignore`, so the trash is never committed. The
[terminal UI](terminal.md) deletes into the same trash.

## History

Every request you send from the UI is kept with its response: status,
headers, time, size, and the body up to its first MiB. The **History**
tab groups them by day and searches them: every word you type must match
the name, URL, method, status or path. Opening an entry opens the request
as a new tab with the response it got; send it to run it again.

History keeps the last 200 requests in `.getman/`. **Clear history**
deletes them and their responses.

## Variables in tables

The Params, Headers, form body, folder header and variable tables mark
`{{variables}}` as the URL bar does, red when no scope defines them.
Typing `{{` offers the names the request can use; <kbd>↑</kbd>
<kbd>↓</kbd> choose and <kbd>Enter</kbd> or <kbd>Tab</kbd> takes one.
Point at a variable, or move the caret into it, to see its value and the
scope it comes from. Values the environment marks secret show as dots
until you choose **Reveal**.

## Header presets

The **Presets** menu beside **Bulk edit** on a Headers tab adds a set of
headers in one go. JSON, Form, XML and No cache come built in; a header
already in the table takes the preset's value instead of being added
twice.

**Save these headers as a preset** keeps the table's headers under a
name, for this workspace on this machine, in
`.getman/header-presets.yaml`. They are kept there rather than in the
browser, whose storage belongs to the port the UI happened to get, and
rather than in a committed file, which would share header values such as
tokens with everyone who clones the repository. The file is plain YAML,
so you can edit it or copy it to another workspace. **Manage presets**
deletes them.

## Response bodies

- **Hex** shows any body as offsets, bytes and ASCII, the first 16 KB at
  once and more on request. Binary bodies open in it.
- Web addresses in the Pretty and Raw views are underlined; click one to
  open it in a new tab.
- **Find** (or <kbd>Ctrl</kbd> <kbd>F</kbd> in the body) searches with
  next and previous, match case, whole word and regular expressions.
- Select a value, such as a token, and choose **Set as variable**. The
  name is suggested from the JSON key on that line, and a JSON string
  loses its quotes. Save it in this request (an unsaved change), the
  environment, the workspace or the globals. The last three go to
  git-ignored local files, where a script's `gm.environment.set`,
  `gm.collectionVariables.set` and `gm.globals.set` would put them.

## Environments

**Duplicate**, next to Rename, copies an environment with its local
values and secrets, as `<name> copy`.
