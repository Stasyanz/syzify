# Contributing to Syzify

Thanks for your interest in the project! Before sending anything, please read
this document — it's short.

## Project model: open source, but not open contribution

Syzify is an open project with a closed development process (the same model
SQLite uses). The code is open under the [GNU AGPL v3](LICENSE) with the
[Syzify Plugin Exception](LICENSE-PLUGIN-EXCEPTION.md): you can read it,
study it, build it and fork it. But **pull requests to this repository are
not accepted** — all core code is written by the project's author, who holds
the entire copyright to the core.

Submitted PRs will be closed without review. That's not rudeness, it's legal
hygiene: sole copyright ownership is what lets the project maintain the
plugin exception and manage its license flexibly.

## What you can and should send: issues

- **Bug reports** — with reproduction steps, the app version and your OS.
  A FIT/GPX file that reproduces the problem speeds things up a lot (make
  sure you're comfortable sharing it — these files contain location data).
- **Ideas and suggestions** — features, UX improvements, oddities you've
  noticed.
- **Questions** — anything unclear about the app or the Plugin API.

Important: by describing an idea in an issue you agree that the project's
author may freely implement it in Syzify without any obligations. Please
don't post finished code or patches in issues — for the reasons above they
won't be used.

## Want to write code? Write a plugin

No permission or signatures needed: thanks to the Plugin Exception, plugins
that work through the official Plugin API are independent works. You own
your plugin entirely and distribute it under any license you choose,
including a commercial one.

- The Plugin API (manifest, host functions, ViewSpec) and example plugins:
  [examples/plugins/](examples/plugins/)
