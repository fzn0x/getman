# Keyboard shortcuts

Every key Getman answers to, in the browser UI and the desktop app, in
their editors and in the terminal UI, grouped by where each one works.

Press <kbd>?</kbd> in the browser UI or the terminal UI to see the main
ones. [Getting started](getting-started.md#keyboard) lists the few worth
learning first.

## Browser UI and desktop app

The desktop app shows the same UI as the browser, so the same keys work
in both. Tab and window keys use <kbd>Alt</kbd> because browsers keep
<kbd>Ctrl</kbd> <kbd>N</kbd>, <kbd>Ctrl</kbd> <kbd>W</kbd> and
<kbd>Ctrl</kbd> <kbd>Tab</kbd> for themselves. While a dialog is open,
such as the command palette, only that dialog's keys work.

### On a Mac

The UI labels keys for the system it runs on. This page writes them as
on Windows and Linux; on a Mac, read them like this:

| Written here | On a Mac |
| --- | --- |
| <kbd>Ctrl</kbd> | <kbd>Cmd</kbd> |
| <kbd>Alt</kbd> | <kbd>Opt</kbd>, the Option key |

So <kbd>Ctrl</kbd> <kbd>K</kbd> is <kbd>Cmd</kbd> <kbd>K</kbd> and
<kbd>Alt</kbd> <kbd>N</kbd> is <kbd>Opt</kbd> <kbd>N</kbd>. The tables
note the few keys that differ more than that.

### Anywhere

These work wherever the focus is, unless the notes say otherwise.

| Keys | Does | Notes |
| --- | --- | --- |
| <kbd>Ctrl</kbd> <kbd>Enter</kbd> | Send the request | In text fields and editors too. WebSocket and Socket.IO: connect, or once connected send the message being written. Event stream: connect. gRPC: start the call, or send the messages while a client stream is open |
| <kbd>Esc</kbd> | Cancel the HTTP request being sent | Unless the key closes something first, such as a list of suggestions |
| <kbd>Ctrl</kbd> <kbd>S</kbd> | Save the open tab | |
| <kbd>Ctrl</kbd> <kbd>K</kbd> or <kbd>Ctrl</kbd> <kbd>P</kbd> | Open the command palette | See [Command palette](#command-palette) |
| <kbd>Alt</kbd> <kbd>N</kbd> | New request | |
| <kbd>Alt</kbd> <kbd>W</kbd> | Close the tab | |
| <kbd>Alt</kbd> <kbd>Shift</kbd> <kbd>T</kbd> | Reopen the tab closed last, with its unsaved changes | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>T</kbd> does the same where the browser lets it through |
| <kbd>Alt</kbd> <kbd>A</kbd> | Search the open and recently closed tabs | See [Searching tabs](browser-ui.md#searching-tabs) |
| <kbd>Alt</kbd> <kbd>]</kbd> | Next tab | After the last tab, back to the first |
| <kbd>Alt</kbd> <kbd>[</kbd> | Previous tab | Before the first tab, on to the last |
| <kbd>Alt</kbd> <kbd>1</kbd> to <kbd>Alt</kbd> <kbd>9</kbd> | Go to the first to the ninth tab | |
| <kbd>Alt</kbd> <kbd>L</kbd> | Put the cursor in the URL | |
| <kbd>Alt</kbd> <kbd>E</kbd> | Open the list of environments | |
| <kbd>Alt</kbd> <kbd>C</kbd> | Copy the request as code | On a request's tab |
| <kbd>Alt</kbd> <kbd>I</kbd> | Import | |
| <kbd>Ctrl</kbd> <kbd>B</kbd> | Show or hide the sidebar | |
| <kbd>Ctrl</kbd> <kbd>J</kbd> | Show or hide the console | |
| <kbd>Ctrl</kbd> <kbd>Z</kbd> | Undo: what the newest notice offers to undo, such as a delete, or else the latest rename or move | Outside a text field |
| <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>Z</kbd> or <kbd>Ctrl</kbd> <kbd>Y</kbd> | Redo the rename or move undone last | Outside a text field. <kbd>Ctrl</kbd> <kbd>Y</kbd> does not redo on a Mac |
| <kbd>/</kbd> | Filter the sidebar | Outside a text field. Opens the sidebar if it is hidden |
| <kbd>?</kbd> | Show the shortcuts | Outside a text field |

### Command palette

<kbd>Ctrl</kbd> <kbd>K</kbd>, <kbd>Ctrl</kbd> <kbd>P</kbd> or the search
box at the top opens it. Type to find requests by name or path, folders,
and commands. Start with `>` to search commands only. With nothing typed,
it shows the five requests sent most recently and the first commands.

| Keys | Does |
| --- | --- |
| <kbd>↑</kbd> <kbd>↓</kbd> | Choose |
| <kbd>Enter</kbd> | Open the request or run the command |
| <kbd>Esc</kbd> | Close |

The commands, with the keys that run them without the palette:

| Command | Keys |
| --- | --- |
| Copy this request as code (on a request's tab) | <kbd>Alt</kbd> <kbd>C</kbd> |
| New request | <kbd>Alt</kbd> <kbd>N</kbd> |
| Search open and recently closed tabs | <kbd>Alt</kbd> <kbd>A</kbd> |
| New WebSocket request, New Socket.IO request, New Event stream request | |
| New gRPC request | |
| Import a collection, API spec, HAR file or cURL command | <kbd>Alt</kbd> <kbd>I</kbd> |
| Export the workspace | |
| Run the whole workspace | |
| Workspace settings | |
| Manage environments | |
| Clear session variables | |
| Show cookies | |
| Switch light and dark theme | |
| Put the response below the request, or beside it | |
| Keyboard shortcuts | <kbd>?</kbd> |
| Open a workspace folder (desktop app) | |
| Settings (desktop app) | |
| Check for updates (desktop app, in copies that update themselves) | |
| Use no environment, Use environment: *name* | |

Each folder also has **Folder settings:** *name* and **Run folder:**
*name*, found by its name or path.

### Tabs

These work on a tab that has the focus, such as after clicking it or
reaching it with <kbd>Tab</kbd>.

| Keys | Does |
| --- | --- |
| <kbd>←</kbd> <kbd>→</kbd> | Switch to the tab beside it |
| <kbd>Shift</kbd> <kbd>←</kbd> <kbd>→</kbd> | Move the tab |
| <kbd>Del</kbd> | Close the tab |
| Menu key or <kbd>Shift</kbd> <kbd>F10</kbd> | Open the tab's menu: close others, close to the right, close all, reopen the closed tab, duplicate, reveal in the sidebar, copy the path |

See [Tabs](browser-ui.md#tabs) for what the tabs keep.

### Sidebar

In the tree of requests, with a row focused:

| Keys | Does |
| --- | --- |
| <kbd>↑</kbd> <kbd>↓</kbd> | Move between rows |
| <kbd>Home</kbd> <kbd>End</kbd> | First row, last row |
| <kbd>Enter</kbd> or <kbd>Space</kbd> | Open a request; open or close a folder |
| <kbd>→</kbd> | Open a folder |
| <kbd>←</kbd> | Close a folder, or go to the folder the row is in |
| <kbd>F2</kbd> | Rename |
| <kbd>Del</kbd> | Delete, to the trash. With several items picked, all of them |
| <kbd>Shift</kbd> <kbd>↑</kbd> <kbd>↓</kbd> | Pick a range |
| <kbd>Ctrl</kbd> <kbd>Space</kbd> | Pick or unpick the focused item. On a Mac, <kbd>Ctrl</kbd> works as well as <kbd>Cmd</kbd> |
| <kbd>Esc</kbd> | Clear the picked items |
| Menu key or <kbd>Shift</kbd> <kbd>F10</kbd> | Open the item's menu |

With the mouse, <kbd>Ctrl</kbd> click (<kbd>Cmd</kbd> click on a Mac)
picks or unpicks an item and <kbd>Shift</kbd> click picks a range.

In the filter box (<kbd>/</kbd>):

| Keys | Does |
| --- | --- |
| <kbd>Enter</kbd> | Open the first match |
| <kbd>↓</kbd> | Move to the matches |
| <kbd>Esc</kbd> | Clear the filter |

On the **History** tab, <kbd>↑</kbd> and <kbd>↓</kbd> move between the
search box and the entries, and <kbd>Esc</kbd> in the search box clears
it. With the sidebar's edge focused, <kbd>←</kbd> and <kbd>→</kbd> make
it narrower or wider, 16 pixels at a time.

### Editors

The body, GraphQL, script and message editors, the gRPC message, the
response body and the other fields of several lines are code editors.
The URL bar and the gRPC address are one-line editors: they take the
same keys, except the ones for several lines. Read-only editors, such as
the response body, take the keys that do not change the text.

| Keys | Does | Notes |
| --- | --- | --- |
| <kbd>Enter</kbd> | In the URL bar: send. In the gRPC address: start the call | One-line editors |
| <kbd>Ctrl</kbd> <kbd>F</kbd> | Find | In the find box, <kbd>Enter</kbd> goes to the next match, <kbd>Shift</kbd> <kbd>Enter</kbd> to the previous one, and <kbd>Esc</kbd> closes it |
| <kbd>F3</kbd> or <kbd>Ctrl</kbd> <kbd>G</kbd> | Next match | With <kbd>Shift</kbd>: previous match |
| <kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>G</kbd> | Go to a line | |
| <kbd>Ctrl</kbd> <kbd>D</kbd> | Select the next occurrence of the selection | |
| <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>L</kbd> | Select every occurrence of the selection | |
| <kbd>Ctrl</kbd> <kbd>Space</kbd> | Show suggestions: `{{variables}}`, and in GraphQL the schema's names | <kbd>Ctrl</kbd> on a Mac too. Typing `{{` shows them as well |
| <kbd>↑</kbd> <kbd>↓</kbd>, <kbd>PgUp</kbd> <kbd>PgDn</kbd> | Choose a suggestion | While suggestions show |
| <kbd>Enter</kbd> | Take the suggestion | While suggestions show |
| <kbd>Esc</kbd> | Close the suggestions | |
| <kbd>Tab</kbd> / <kbd>Shift</kbd> <kbd>Tab</kbd> | Indent / outdent | |
| <kbd>Esc</kbd>, then <kbd>Tab</kbd> | Move the focus out of the editor | Within two seconds |
| <kbd>Ctrl</kbd> <kbd>M</kbd> | Switch <kbd>Tab</kbd> between indenting and moving the focus | <kbd>Shift</kbd> <kbd>Alt</kbd> <kbd>M</kbd> on a Mac |
| <kbd>Ctrl</kbd> <kbd>Z</kbd> | Undo in the editor | |
| <kbd>Ctrl</kbd> <kbd>Y</kbd> | Redo in the editor | <kbd>Cmd</kbd> <kbd>Shift</kbd> <kbd>Z</kbd> on a Mac. <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>Z</kbd> also works on Linux |
| <kbd>Alt</kbd> <kbd>↑</kbd> <kbd>↓</kbd> | Move the line up or down | |
| <kbd>Shift</kbd> <kbd>Alt</kbd> <kbd>↑</kbd> <kbd>↓</kbd> | Copy the line up or down | |
| <kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>↑</kbd> <kbd>↓</kbd> | Add a cursor above or below | |
| <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>K</kbd> | Delete the line | |
| <kbd>Ctrl</kbd> <kbd>[</kbd> / <kbd>Ctrl</kbd> <kbd>]</kbd> | Outdent / indent the line | |
| <kbd>Ctrl</kbd> <kbd>/</kbd> | Comment or uncomment the line | Where the language has comments, such as scripts and XML |
| <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>[</kbd> / <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>]</kbd> | Fold / unfold the block at the cursor | <kbd>Cmd</kbd> <kbd>Opt</kbd> <kbd>[</kbd> / <kbd>]</kbd> on a Mac |
| <kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>[</kbd> / <kbd>Ctrl</kbd> <kbd>Alt</kbd> <kbd>]</kbd> | Fold / unfold everything | <kbd>Ctrl</kbd> <kbd>Opt</kbd> on a Mac, not <kbd>Cmd</kbd> |
| <kbd>Ctrl</kbd> click | On a field in a GraphQL query: show it in **Docs** | <kbd>Cmd</kbd> click on a Mac |

The arrow, <kbd>Home</kbd>, <kbd>End</kbd> and <kbd>PgUp</kbd>
<kbd>PgDn</kbd> keys move and, with <kbd>Shift</kbd>, select, as in other
text editors. The keys under [Anywhere](#anywhere) work in the editors
too, except undo, redo, <kbd>/</kbd> and <kbd>?</kbd>, which edit or
type there.

The fields of the Params, Headers, form body and variable tables offer
`{{variables}}` too: <kbd>↑</kbd> <kbd>↓</kbd> choose, <kbd>Enter</kbd> or
<kbd>Tab</kbd> takes one, and <kbd>Esc</kbd> closes the list or the
value shown for a variable.

### Console

| Keys | Does |
| --- | --- |
| <kbd>Ctrl</kbd> <kbd>J</kbd> | Show or hide the console |
| <kbd>↑</kbd> <kbd>↓</kbd>, <kbd>Home</kbd> <kbd>End</kbd> | In the list: choose a line, or the first or last one |
| <kbd>Esc</kbd> | Close the console, unless the focus is in its filter box or on **Errors only** |
| <kbd>↑</kbd> / <kbd>↓</kbd> | On its top edge: make it taller or shorter, 24 pixels at a time |

See [Console](browser-ui.md#console).

### Requests and runs

| Where | Keys | Does |
| --- | --- | --- |
| The divider between request and response | <kbd>←</kbd> <kbd>↑</kbd> / <kbd>→</kbd> <kbd>↓</kbd> | Move it, by 5% of the width or height |
| The log of a WebSocket, event stream or Socket.IO connection | <kbd>↑</kbd> <kbd>↓</kbd> | Choose a message and show it in full |
| The timing panel | <kbd>Esc</kbd> | Close it |
| The handle of a request in the Runner | <kbd>↑</kbd> <kbd>↓</kbd> | Move the request earlier or later in the run, while it is not running |

### Dialogs and menus

| Keys | Does |
| --- | --- |
| <kbd>Esc</kbd> | Close the dialog without doing anything. A click outside it does the same |
| <kbd>Enter</kbd> | In a dialog that asks for a name, such as Rename or Save as: confirm |
| <kbd>↑</kbd> <kbd>↓</kbd> | In a menu: move between items |
| <kbd>Esc</kbd> or <kbd>Tab</kbd> | Close the menu |

A dialog that asks you to confirm, such as before a delete, starts with
the focus on **Cancel**.

## Desktop app menus

On macOS the desktop app has the standard menu bar, with these keys. On
Windows and Linux its window has no menu bar.

| Menu | Item | Keys |
| --- | --- | --- |
| Getman | About Getman, Services | |
| | Hide Getman | <kbd>Cmd</kbd> <kbd>H</kbd> |
| | Hide Others | <kbd>Cmd</kbd> <kbd>Opt</kbd> <kbd>H</kbd> |
| | Quit Getman | <kbd>Cmd</kbd> <kbd>Q</kbd> |
| File | Close Window | <kbd>Cmd</kbd> <kbd>W</kbd> |
| Edit | Undo | <kbd>Cmd</kbd> <kbd>Z</kbd> |
| | Redo | <kbd>Cmd</kbd> <kbd>Shift</kbd> <kbd>Z</kbd> |
| | Cut, Copy, Paste, Select All | <kbd>Cmd</kbd> <kbd>X</kbd>, <kbd>Cmd</kbd> <kbd>C</kbd>, <kbd>Cmd</kbd> <kbd>V</kbd>, <kbd>Cmd</kbd> <kbd>A</kbd> |
| View | Toggle Full Screen | <kbd>Ctrl</kbd> <kbd>Cmd</kbd> <kbd>F</kbd> |
| Window | Minimize | <kbd>Cmd</kbd> <kbd>M</kbd> |
| | Zoom | |
| | Close Window | <kbd>Cmd</kbd> <kbd>W</kbd> |

The app has no keys of its own beyond these and the UI's. Getman's tab
keys use <kbd>Opt</kbd>, so <kbd>Cmd</kbd> <kbd>W</kbd> closes the
window, not a tab.

## Terminal UI

Press <kbd>?</kbd> in `getman tui` for a summary; any key closes it.
<kbd>Ctrl</kbd> <kbd>C</kbd> quits at once, from anywhere, even with
unsaved changes. [Terminal UI](terminal.md) explains each part.

### Any pane

These work in the tree, the request and the response, when you are not
typing a line.

| Keys | Does |
| --- | --- |
| <kbd>Tab</kbd> / <kbd>Shift</kbd> <kbd>Tab</kbd> | Move between the tree, the request and the response. With no request open, the focus stays on the tree |
| <kbd>s</kbd> | Send the open request, or connect or call |
| <kbd>Esc</kbd> or <kbd>x</kbd> | Stop what is running: a request, a connection, a call or a sign-in |
| <kbd>w</kbd> | Save the open request |
| <kbd>e</kbd> | Next environment |
| <kbd>E</kbd> | Previous environment. In the request pane, <kbd>E</kbd> edits instead |
| <kbd>/</kbd> | Filter requests by name or path |
| <kbd>r</kbd> | Run the row selected in the tree, when the tree has the focus; else the whole workspace |
| <kbd>R</kbd> | Reload the tree from disk |
| <kbd>?</kbd> | Show the keys |
| <kbd>q</kbd> | Quit. With unsaved changes, press it twice |
| <kbd>Ctrl</kbd> <kbd>C</kbd> | Quit at once |

### Tree

| Keys | Does |
| --- | --- |
| <kbd>j</kbd> <kbd>k</kbd> or <kbd>↓</kbd> <kbd>↑</kbd> | Move |
| <kbd>g</kbd> <kbd>G</kbd> or <kbd>Home</kbd> <kbd>End</kbd> | First, last |
| <kbd>Enter</kbd>, <kbd>l</kbd>, <kbd>o</kbd> or <kbd>→</kbd> | Open a request; open or close a folder |
| <kbd>h</kbd> or <kbd>←</kbd> | Close a folder, or go to the folder the row is in |
| <kbd>a</kbd> | Add a request in the selected folder |
| <kbd>d</kbd> or <kbd>Del</kbd> | Move the selected item to the trash, after <kbd>y</kbd> confirms |
| <kbd>u</kbd> | Put back what you moved to the trash last |

### Request pane

| Keys | Does |
| --- | --- |
| <kbd>i</kbd> or <kbd>Enter</kbd> | Edit the URL |
| <kbd>m</kbd> / <kbd>M</kbd> | Next or previous method. HTTP requests only |
| <kbd>1</kbd> to <kbd>5</kbd> | Params, headers, body, auth, checks |
| <kbd>]</kbd>, <kbd>l</kbd> or <kbd>→</kbd> | Next part |
| <kbd>[</kbd>, <kbd>h</kbd> or <kbd>←</kbd> | Previous part |
| <kbd>E</kbd> or <kbd>o</kbd> | Edit the part shown in your editor |
| <kbd>b</kbd> | On the auth part: sign in to OAuth 2.0 with your browser |

### Response pane

| Keys | Does |
| --- | --- |
| <kbd>j</kbd> <kbd>k</kbd> or <kbd>↓</kbd> <kbd>↑</kbd> | Scroll a line |
| <kbd>PgDn</kbd> or <kbd>Space</kbd>, <kbd>PgUp</kbd> | Scroll 20 lines |
| <kbd>g</kbd> or <kbd>Home</kbd> | Top |
| <kbd>G</kbd> or <kbd>End</kbd> | End. On a connection's log, follow new messages again |
| <kbd>1</kbd> to <kbd>4</kbd> | Body, headers, tests, console |
| <kbd>]</kbd>, <kbd>l</kbd> or <kbd>→</kbd> | Next tab |
| <kbd>[</kbd>, <kbd>h</kbd> or <kbd>←</kbd> | Previous tab |

### Connections and calls

These work on an open WebSocket, event stream, Socket.IO or gRPC
request, wherever the focus is.

| Keys | Does |
| --- | --- |
| <kbd>m</kbd> | WebSocket and Socket.IO: choose a saved message to send. gRPC: send the request's message |
| <kbd>t</kbd> | Type a message to send |
| <kbd>f</kbd> | gRPC: end the stream you send |
| <kbd>x</kbd> | Disconnect, or cancel the call |

An event stream only receives, so <kbd>m</kbd> and <kbd>t</kbd> say so
and do nothing.

### Typing a line

The URL, the filter, a new request's name and a message are typed on one
line.

| Line | Keys | Does |
| --- | --- | --- |
| URL | <kbd>Enter</kbd> | Stop editing and send |
| | <kbd>Esc</kbd> | Stop editing |
| | <kbd>←</kbd> <kbd>→</kbd>, <kbd>Home</kbd> <kbd>End</kbd> | Move the cursor |
| | <kbd>Backspace</kbd>, <kbd>Del</kbd> | Delete before or after the cursor |
| | <kbd>Ctrl</kbd> <kbd>U</kbd> | Clear the URL |
| Filter | <kbd>↑</kbd> <kbd>↓</kbd> | Move in the tree |
| | <kbd>Enter</kbd> | Keep the filter, and open the selected request |
| | <kbd>Esc</kbd> | Clear the filter |
| New request (<kbd>a</kbd>) | <kbd>Enter</kbd> | Create it, open it and edit its URL |
| | <kbd>Esc</kbd> | Cancel |
| Message (<kbd>t</kbd>) | <kbd>Enter</kbd> | Send |
| | <kbd>Esc</kbd> | Close the line without sending |
| | <kbd>Ctrl</kbd> <kbd>U</kbd> | Clear the line |

### Lists and screens

| Where | Keys | Does |
| --- | --- | --- |
| Saved messages (<kbd>m</kbd>) | <kbd>j</kbd> <kbd>k</kbd> or <kbd>↓</kbd> <kbd>↑</kbd>, then <kbd>Enter</kbd> | Choose one and send it |
| | <kbd>1</kbd> to <kbd>9</kbd> | Send that one |
| | <kbd>Esc</kbd> or <kbd>q</kbd> | Close the list |
| Delete (<kbd>d</kbd>) | <kbd>y</kbd> | Move it to the trash |
| | Any other key | Keep it |
| A run (<kbd>r</kbd>) | <kbd>j</kbd> <kbd>k</kbd> or <kbd>↓</kbd> <kbd>↑</kbd> | Scroll |
| | <kbd>Esc</kbd> or <kbd>q</kbd> | Close the run, and stop it if it is still going |
| The keys (<kbd>?</kbd>) | Any key | Close |

## Command line

| Command | Keys | Does |
| --- | --- | --- |
| `getman`, `getman open` | <kbd>Ctrl</kbd> <kbd>C</kbd> | Stop the server |
| `getman run` | <kbd>Ctrl</kbd> <kbd>C</kbd> | Stop the run after the request that is running |
| `getman send` on a connection | <kbd>Ctrl</kbd> <kbd>C</kbd> | Disconnect. A second one stops waiting and exits with status 130 |
| `getman mock` | <kbd>Ctrl</kbd> <kbd>C</kbd> | Stop the mock server |

See [Command line](cli.md) for every command.
