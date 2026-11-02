# Keeping invoices in git

Invoices are small text files, so a git repository gives you history, backups
and a record of when each invoice was issued and paid.

## Set up

Keep the markdown in git and treat the PDFs as build output that can always be
regenerated:

```sh
cd ~/Documents/Invoices
git init
printf '*.pdf\n' > .gitignore
git add .gitignore *.md
git commit -m "Invoices so far"
```

If you send PDFs to clients and want the exact file you sent on record, commit
the PDFs too instead of ignoring them. The output for a given markdown file and
config only changes when invgen's template changes.

Consider keeping the config next to the invoices so the two travel together,
and point invgen at it:

```sh
mv ~/.config/invgen/config.yaml ~/Documents/Invoices/config.yaml
export INVGEN_CONFIG=~/Documents/Invoices/config.yaml   # in your shell profile
```

The config holds your bank details and client addresses, so keep such a
repository private.

## Organize by year

The next-number scan and `invgen list` both look two levels deep, so per-year
folders work:

```text
Invoices/
  config.yaml
  2025/EINV-2025-12.md
  2025/EINV-2025-13.md
  EINV-2026-14.md
```

`invgen new` and the editor create new files at the top of `invoices_dir`, and
the editor sidebar lists only that top level, so move older years into their
folders once they are closed.

## Check before committing

`invgen list` exits with status 1 when any invoice fails to parse, so it works as
a pre-commit check:

```sh
# .git/hooks/pre-commit (make it executable)
#!/bin/sh
exec invgen list >/dev/null
```

## Commit messages that tell the story

A commit per event keeps `git log` readable as a ledger:

```sh
git add EINV-2026-23.md && git commit -m "Issue EINV-2026-23 (Northwind, October)"
git commit -am "EINV-2026-23 paid"
```
