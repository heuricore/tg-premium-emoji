# Roadmap

Items deliberately left out of v1.

## Third-party catalog install

Install a third-party catalog by command, with safety checks: a file whitelist (json, md, png only), path traversal and symlink rejection, PNG and size limits, and showing the contents with a confirmation before anything is written. In v1 the user drops a third-party catalog in as a folder and enables it explicitly in `config.json`.

## Catalog download

Download catalogs over the network: trust model, signatures and versioning.

## Style export to language modules

Export a style into a module in the project's language (`.rs`, `.ts`, `.py`). v1 offers `tge style resolve --json`, whose output a project can read as a file.

## Framework how-to skills

Skills that explain applying the result in aiogram, gramio, telers (Rust) and the raw Bot API. v1 stays independent of any bot library and outputs generic snippets only.

## Image to emoji

Turn an image into a set of emoji: slicing it into a new pack, or building a mosaic from existing packs.

## claude.ai and Cowork support

Support the plugin outside Claude Code, in claude.ai and Cowork. v1 targets Claude Code only.
