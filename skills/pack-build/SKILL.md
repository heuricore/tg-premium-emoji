---
name: pack-build
description: Build a Telegram custom emoji pack into the tge catalog and label every emoji so it can be found by meaning. Use when the user shares a t.me/addemoji link or a set name, wants to add, import, scan or rescan a premium emoji pack, asks to label, describe, tag or relabel emoji, dictates what an emoji means, wants to fix a wrong label, or asks what is left to label. Also use when tge find reports unlabeled emoji in a pack, or when a pack fails tge validate.
---

# pack-build: from a set name to a labeled pack

## Before the first `tge` command: is tge available?

The binary lives at `${TG_PREMIUM_EMOJI_HOME:-$HOME/.tg-premium-emoji}/bin/latest/tge`
(`tge.exe` on Windows) and is not on PATH; below, `tge` stands for that full path. Write the full
path in every command: shell variables do not survive between commands in most agents.

- Check that it exists and that `tge --version` is not older than `../../VERSION`. Paths here are
  relative to this skill's folder (its base directory), not the current directory.
- Missing or older: tell the user, ask, then run `python3 ../../scripts/install.py` (`--update`
  when an older one is installed; `python` on Windows) or let the user run it.
- install.py prints `newer: ...`: offer the update. Never install or update silently.

Rules, catalogs, exit codes and how to read preview sheets: the `guide` skill.

## Why the labels matter

Selection later runs only on the text you write here: `tge find` searches names, tags and
descriptions, and emoji without a confidence are invisible to it. A vague label makes the agent
that picks emoji guess again, which is the problem this plugin exists to remove. Describe what you
see, invent nothing, and say how sure you are.

Read `references/labeling.md` before the first labeling batch of a session: it defines good
names, distinguishing descriptions, tag rules, every `fits` value and the confidence levels.

## Prerequisite: BOT_TOKEN

`tge pack build` downloads the set through the Telegram Bot API and reads `BOT_TOKEN` from the
environment. Run `tge doctor` and look at the `bot_token:` line.

- `not set`: ask the user to export `BOT_TOKEN` in their shell and restart the agent session from
  that shell. Do not ask them to paste the token into the chat, and never put it in a command
  (`BOT_TOKEN=... tge` ends up in shell history and the session log).
- Labeling, preview, similar, report and validate need no token. A pack that is already in a
  catalog can be labeled without one.

## 1. Build

The set name is the last part of `t.me/addemoji/<short_name>`.

```bash
tge pack build <short_name>
```

It writes to the personal `local` catalog; add `--catalog project` to write into the project's
`.tg-premium-emoji/` instead. To work on a catalog folder directly (for example the repository
source of the built-in catalog), pass the global `--catalog-dir <dir>` to every command of the
session. It prints counts: new, changed (image replaced), gone, returned, total, and any emoji
that failed to decode.

Rebuilding the same pack is a rescan, safe at any time: labels are kept and matched by id. New
emoji arrive unlabeled. An emoji whose image was replaced keeps its labels but drops to `low` (unless
a human labeled it), so look at it again. Emoji that left the set are marked gone and never found.

## 2. Preview

```bash
tge pack preview <short_name>
```

It prints the absolute path of every sheet with its `n` range (`..._1.png  1-20`). Open each
sheet with your file-reading tool when you label it, one sheet at a time.

## 3. Pass 1: names, tags, fits, confidence per sheet

For each sheet, write one batch of edits covering every emoji on it and apply it:

```bash
tge label <short_name> --edits - <<'EOF'
[
  {"n": 1, "name": "check mark in circle", "tags": ["галочка", "готово", "успех", "check", "done", "success"], "fits": ["status", "button"], "confidence": "high"},
  {"n": 2, "name": null, "confidence": "low"}
]
EOF
```

`--edits` takes a file path or `-` for stdin; a file written with your file tool works the same
and avoids shell quoting issues on Windows. Each edit names exactly one of `n` or `id`. `name`,
`description`, `fits` and `confidence` are tri-state: absent keeps the field, `null` clears it, a
value sets it. `tags` replaces the whole list.

- Always send `confidence` with labels. An edit that leaves an emoji with a name, description,
  tags or fits but no confidence is rejected (exit 3) and nothing is written.
- Never send an edit with only `n` or `id`: it changes nothing but still marks who labeled it.
- Not recognized: `"name": null, "confidence": "low"`. Invent nothing; a guessed name sends the
  emoji to the wrong searches, while `low` puts it in the queue for a human.
- Pass 1 needs no `description`; that is pass 2's job.

The command prints what it applied, what it skipped because a human labeled it, and what it did
not find. On exit 3 it lists the issues: fix the batch and resend it. If an issue points at a part
of the pack your batch did not touch, the pack already had a problem: run `tge validate`, report
it to the user, and do not hand-edit `pack.json`.

## 4. Similar groups

```bash
tge similar <short_name>
```

It groups emoji whose names and tags overlap. Only emoji with a name take part, so run it after
pass 1. These groups are where the next agent would confuse one emoji with another.

## 5. Pass 2: distinguishing descriptions per group

For each group, open the sheet or sheets that hold its numbers and look at the members side by
side. Write a `description` for each that tells it apart from the others in the group: colour,
shape, detail, direction, state. Refine a name or tags when the comparison shows they were too
generic. Send one batch per group, with `confidence` again for any emoji whose certainty changed.

A short description also helps for emoji outside any group, but groups come first.

## 6. Report

```bash
tge report <short_name>
```

It lists what is left: `unlabeled`, `low`, `frame_suspect`, `gone`. Inspecting single emoji at a
larger size (`tge inspect`) arrives in Phase 2, so for now a doubtful or `frame_suspect` emoji you
cannot read from the sheet stays `low`. Show the user the queue with numbers; niche brand logos
landing there is expected. Offer to take their answers (see "Labels a human dictates").

## 7. Validate

```bash
tge validate
```

Expect exit 0 and `ok`. Exit 3 lists the issues with the file they are in.

## Labels a human dictates

When the user tells you what an emoji is, send their label with `--by human`:

```bash
tge label <short_name> --by human --edits - <<'EOF'
[{"n": 17, "name": "Fragment logo", "tags": ["fragment", "фрагмент"], "confidence": "high"}]
EOF
```

`by: human` protects the emoji: later agent edits are skipped and reported as such. Use
`--force` only when the user explicitly asks to overwrite their own labels with yours.

## Finish

Tell the user the pack's totals, how many emoji are `high`, `medium` and `low`, what is in the
report queue, and that `emoji-design` can now search the pack.
