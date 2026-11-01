---
name: emoji-design
description: Pick Telegram premium (custom) emoji for bot messages, keyboard buttons, channel posts and status lines by meaning, and hand over ready snippets (HTML tg-emoji tag, MarkdownV2, button icon_custom_emoji_id). Use whenever the user wants emoji in a Telegram bot or channel, asks which emoji fits a success, error, back, settings or any other element, needs a custom_emoji_id, wants to decorate, restyle or review messages, buttons or posts, or pastes Telegram HTML with tg-emoji tags, even when they do not say premium or custom.
---

# emoji-design: the right emoji for each element

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

## What this skill does and does not do

It designs: which emoji goes on which element, and the exact snippet for it. How the user sends
the result (aiogram, gramio, telers, raw Bot API, a userbot, a channel) is their business, so hand
over generic snippets and do not write delivery code unless asked.

The rule that matters most: **ids come only from `tge` output.** Never type an id from memory,
never adjust one, never carry one over from another pack. A wrong id shows a different emoji or
none at all, and nobody notices until a user sees it.

## Start: ask about a style

Ask first whether there is a style for this bot or channel, or a pack the user wants to stick to.
A style fixes which emoji fills each role (success, error, back, ...) so nothing is guessed.

Styles arrive in Phase 2 and `tge` has no style commands yet. Until then:

- The user names a pack: search only in it (`--pack <short_name>`).
- No style and no pack: you pick per element (mode 1) and keep one look across the screen or post.
- The user describes a style in words ("thin white line icons"): treat it as a constraint in mode 1.

## Mode 1: you choose

For every element (a message line, a heading, a button, a status, a list marker):

1. **State what it means** in a few words before searching: "confirms the order was paid", "goes
   back to the main menu". Searching for the meaning, not for a word in the label, is what keeps
   the pick right.
2. **Search** with the meaning words, in English or Russian:
   ```bash
   tge find order paid success --fits status
   ```
   Useful filters: `--fits text|heading|list_item|button|status|divider`, `--pack`,
   `--repaint true` (monochrome emoji that take the text colour), `--animated true|false`
   (whether it moves), `--format static|tgs|webm`,
   `--limit N` (default 10). No hits: try synonyms, the other language, fewer filters.
3. **Choose by meaning, not by rank.** Read every hit's name, description and confidence (and
   `fits` in the JSON); the first hit only matched the words best. Prefer `high` over `medium`;
   check a `low` one on its preview before using it. When two candidates are close, compare their
   descriptions first and look at the preview sheet only if you are still unsure (see "When in
   doubt").
4. **Leave it out when nothing fits.** No emoji is better than a misleading one; say so in the
   table.
5. **One emoji per element.** A button gets one icon; a line gets one marker.
6. **One look per screen or post.** Keep the same visual family (same pack where possible, same
   stroke, filled or outline, monochrome or colour) across everything shown together.

End with the table and the snippets (see "Hand-over").

## Mode 2: the user named the emoji

The user names an emoji from a catalog (pack and number, or an id from earlier `tge` output):
use it as is. Get its snippet with `tge snippet <pack> <n|id>`. A raw id the user typed and that
no catalog holds is theirs to vouch for: copy it exactly, and say it is not in the catalog.
Slots of a style (`style:slot`) arrive in Phase 2.

To inspect an emoji (labels, flags, preview cell) run `tge show <pack> <n|id>`; do not read
`pack.json`. The `animated` field, not `format`, says whether it moves: `format: tgs` is only a
Lottie file and may be still.

## Mode 3: mixed

Use the named emoji as in mode 2 and choose the rest as in mode 1, matching the look of the ones
the user fixed.

## When in doubt: escalate

1. **Text catalog**: the `tge find` output, names, descriptions and confidence.
2. **One emoji's full record**: `tge show <pack> <n|id>` prints every label and flag plus the
   preview sheet, row and column where it sits.
3. **Preview sheet**: open that sheet with your file-reading tool and compare the candidate with
   its neighbours.
4. Inspecting one emoji at a larger size (`tge inspect`) arrives in Phase 2.

A description that says "an identical-looking twin exists in this pack" means two emoji render
the same; either one is fine, but use the same one everywhere on the screen or post.

`pack X: N unlabeled` under the hits means part of that pack is invisible to search. Mention it,
and offer the `pack-build` skill to finish labeling when the pack matters to the user. The same
skill adds a new pack when no catalog has what the user needs.

## Snippets

`tge find` prints the HTML snippet under each hit; `tge snippet` gives all three forms for one
emoji:

```bash
tge snippet <pack> <n|id> --text "Back"
```

```
html: <tg-emoji emoji-id="ID">FALLBACK</tg-emoji>
markdown_v2: ![FALLBACK](tg://emoji?id=ID)
button: {"icon_custom_emoji_id":"ID","text":"Back"}
```

Copy these strings from the output; do not rebuild them by hand. `--text` is the button label,
without any emoji in it: the icon replaces it. Add `"style": "danger" | "success" | "primary"` to
a button only when the user asks for a coloured button.

Read `references/bot-api.md` before writing the final message text or keyboard, and whenever the
user asks where premium emoji work: it has the escaping rules and the places (callback alerts,
bot description, command list, menu button) where only plain emoji work.

## Hand-over

Answer in the user's language. Always end with this table, one row per element (`ID` stands for
the full id copied from `tge` output):

| Element | Emoji | Why |
|---|---|---|
| Payment confirmed (first line) | `tgiosicons` #65 ✅ check mark in circle, id `ID` | Confirms success; monochrome outline like the other status icons, takes the text colour |
| "Back" button | none | No arrow in the pack reads clearly as back; plain text label |

Then give the snippets: the message text with the `tg-emoji` tags in place (HTML) or the
MarkdownV2 variant if the user uses MarkdownV2, and the keyboard as button JSON.
