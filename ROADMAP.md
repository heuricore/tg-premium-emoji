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

## Project-first storage

A project keeps its own packs and styles in its folder, ahead of the user's home:

- Styles (Phase 2) live in the project catalog next to its packs and resolve project first, then home.
- When a project catalog exists, write commands (`pack build`, `label`, styles) target it by default instead of home.
- A command that creates the project catalog on purpose, so the skills can offer it at the start of a project.

## Create your own packs

A skill and `tge` commands that let the agent create a custom emoji set itself, for example icons in the theme of the user's app: design the emoji (draw SVG, render, convert), then publish the set through the Bot API (`createNewStickerSet` with `sticker_type: custom_emoji`, `uploadStickerFile`, `addStickerToSet`; static emoji are 100×100 WEBP or PNG; the set name must end in `_by_<bot username>` and the set belongs to a user who has started the bot). The new set then goes through `pack build` and labeling like any other. Builds on "Image to emoji".

## Image to emoji

Turn an image into a set of emoji: slicing it into a new pack, or building a mosaic from existing packs.

## Other agents

Run the skills in local agents besides Claude Code, such as Codex and OpenCode. The `tge` binary and its state in `~/.tg-premium-emoji` already work with any agent, and the skills are plain `SKILL.md` files. Missing: an install path for each agent (where it looks for skills, and how a skill finds `scripts/install.py` and `VERSION` from there), a pass over the skill text for anything that only Claude Code provides, and a test run in each agent.
