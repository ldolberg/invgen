# The editor

`invgen` (or `invgen edit FILE`) opens a native window with three parts: the
invoice library on the left, the markdown editor in the middle and a live
preview of the PDF on the right.

![invgen editor](https://raw.githubusercontent.com/ldolberg/invgen/main/docs/screenshot.png)

## Keyboard shortcuts

| Keys | Action |
|---|---|
| Ctrl+N | New invoice with the next number |
| Ctrl+O | Open an invoice |
| Ctrl+S | Save |
| Shift+Ctrl+S | Save as |
| Ctrl+E | Export the PDF next to the markdown file |
| Shift+Ctrl+E | Export the PDF to a location you choose |
| Ctrl+B | Show or hide the invoice library |
| Ctrl+= (or Ctrl++) | Zoom the preview in |
| Ctrl+- | Zoom the preview out |
| Ctrl+0 | Fit the preview to the panel width |
| Ctrl+mouse wheel | Zoom the preview around the pointer |

The toolbar buttons do the same things and show their shortcut on hover.

## Writing

The editor highlights the front matter, headings, tables, lists and numbers.
Write the invoice as described in [Invoice format](format.md); a new document
(Ctrl+N) starts from the same template as `invgen new`, for the first client in
your config.

Below the editor a status line reports the state of the preview:

- **Preview up to date** when the text parses and renders.
- **Line N: message** when there is a mistake. The line is highlighted in the
  editor; click the message to jump to it.

The window title shows a dot in front of the file name while there are unsaved
changes.

## Live preview

The preview re-renders shortly after you stop typing. It shows exactly what the
exported PDF will look like. While the text has an error, the last good preview
stays on screen with a banner that repeats the error, so the page does not
flicker while you type.

Above the preview a bar shows the invoice total, number, status and client, the
zoom level, a **Fit width** button and the page count. Drag the divider between
editor and preview to resize them; double click it to restore the default split.

## Saving and exporting

- **Save** (Ctrl+S) writes the markdown. A new, untitled invoice is saved
  straight into `invoices_dir` as `<number>.md` when that name is free;
  otherwise a save dialog opens.
- **Export PDF** (Ctrl+E) writes `<number>.pdf` next to the markdown file (or into
  `invoices_dir` for an untitled invoice). The file name always follows the
  number in the current text. Right click the button, or press Shift+Ctrl+E, to
  pick another location.
- **Open PDF** opens the last export in your PDF viewer. The notification shown
  after an export has an **Open** button too.

Opening another file, starting a new one or closing the window with unsaved
changes asks whether to **Save**, **Discard** or **Cancel**.

## Invoice library

The sidebar lists the `.md` files directly inside `invoices_dir`, newest first,
with number, client, total and a colored status. Hover an entry to see its path
and date; click it to open it. Files that fail to parse are still listed, marked
"could not parse". The list refreshes when files in the folder change, including
changes made by other programs.

Toggle the sidebar with Ctrl+B or the button in the top left corner.

## Files changed outside the editor

If the open file changes on disk (for example after a `git pull`, or an edit in
another editor), invgen reloads it. If you have unsaved edits, it keeps your
edits and tells you the file changed on disk instead.

## Drag and drop

Drop a `.md`, `.markdown` or `.txt` file onto the window to open it.

## Themes and remembered settings

The theme button cycles between dark, light and following the system theme. The
editor remembers the theme, the split position, the zoom, whether the sidebar is
shown, the window size and the last opened file.

## Config

The editor reads the config once at startup. A broken config is reported in a
notification and the editor falls back to defaults. Restart the editor after
editing the config. It honors `--config` and `INVGEN_CONFIG` like the CLI.
