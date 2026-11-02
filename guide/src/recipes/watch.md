# Watch mode with your own editor

If you prefer Vim, Emacs, VS Code or any other editor over the built-in one,
`invgen watch` gives you the same edit and preview loop.

```sh
invgen watch ~/Documents/Invoices/EINV-2026-23.md
```

It builds the PDF once, then again every time the file is saved:

```text
10:41:07 built EINV-2026-23  Northwind Labs SL  EUR 9,900.00  /home/alex/Documents/Invoices/EINV-2026-23.pdf
watching /home/alex/Documents/Invoices/EINV-2026-23.md (Ctrl+C to stop)
10:42:15 built EINV-2026-23  Northwind Labs SL  EUR 9,450.00  /home/alex/Documents/Invoices/EINV-2026-23.pdf
```

Open the PDF in a viewer that reloads on change (for example Evince, Okular or
zathura) next to your editor, and every save updates the page.

Details:

- Mistakes are printed as `file:line: error: message` and watching continues;
  the last good PDF stays in place.
- The config is read again on every rebuild, so you can tune the config and the
  invoice at the same time.
- Editors that save by writing a temporary file and renaming it are handled.
- `-o OUT.pdf` writes the PDF somewhere else, for example a fixed preview file
  that your viewer keeps open:

  ```sh
  invgen watch EINV-2026-23.md -o ~/preview.pdf
  ```

## Jumping to errors

Because errors use the `file:line:` format, editors that understand compiler
output can jump to them. In Vim:

```vim
:set makeprg=invgen\ build\ %
:make
```
