# tg-premium-emoji

A Claude Code plugin and a Rust CLI, `tge`, for designing Telegram bots and channels with premium (custom) emoji. Packs are rendered into images, labeled once in text, and then searched by meaning so an agent picks the right emoji instead of guessing from a fallback character.

Status: early development (v0.0.1). The design is in `docs/superpowers/specs/2026-11-01-tg-premium-emoji-design.md`; planned later work is in `ROADMAP.md`.

## What it does

The Bot API gives an agent only a `custom_emoji_id` and a plain fallback emoji for each custom emoji, and one pack often holds several emoji with the same fallback. An agent that cannot see them picks one at random. This plugin fixes that:

1. `tge pack build` downloads a custom emoji set through the Bot API and renders numbered preview sheets.
2. The agent looks at the sheets once and labels every emoji in text: a name, distinguishing descriptions for look-alikes, Russian and English tags, where it fits in an interface, and how sure it is.
3. When designing a message, a keyboard or a post, the agent searches that text catalog by meaning (`tge find`), picks one emoji per element, and hands over ready snippets: an HTML `<tg-emoji>` tag, MarkdownV2, a button `icon_custom_emoji_id`.

The plugin helps design. How you deliver the result (aiogram, gramio, telers, raw Bot API, a userbot, a channel) is up to you.

## Skills

| Skill | Job |
|---|---|
| `guide` | The map: what the plugin does, where data lives, catalogs and priority, running `tge`, global rules |
| `emoji-design` | Picking emoji for messages, buttons and posts by meaning; snippets; a final "element, emoji, why" table |
| `pack-build` | Building a pack from a `t.me/addemoji/<name>` link, two-pass labeling, the labeling report, validation |

Styles, compositions, catalog management and the `catalog-manage` skill come in Phase 2 (see `ROADMAP.md`).

## Install

In a Claude Code session:

```
/plugin marketplace add heuricore/marketplace
/plugin install tg-premium-emoji@heuricore-marketplace
```

From a shell: `claude plugin marketplace add heuricore/marketplace`, then `claude plugin install tg-premium-emoji@heuricore-marketplace`. Skills then appear as `/tg-premium-emoji:guide` and so on; the agent also picks them up on its own when you talk about Telegram premium emoji.

### The `tge` binary

The skills run `tge`, a single binary downloaded from this repository's GitHub Releases (sha256-verified) by `scripts/install.py`. The agent asks before running it; you can also run it yourself with Python 3.8+ (standard library only):

```bash
python3 scripts/install.py            # install the latest release, or report whether newer ones exist
python3 scripts/install.py --check    # only report the installed and newer versions
python3 scripts/install.py --update   # install the newest release and switch to it
```

On Windows use `py` or `python`. In an installed plugin the script is at `~/.claude/plugins/cache/heuricore-marketplace/tg-premium-emoji/<version>/scripts/install.py`. The binary lands in `~/.tg-premium-emoji/bin/<version>/` with `bin/latest` pointing at the active version; it is not put on PATH. It also works from a plain terminal: `~/.tg-premium-emoji/bin/latest/tge --help`.

When the plugin is updated and needs a newer `tge`, the agent tells you and asks before updating. `tge doctor --prune` removes old versions.

### BOT_TOKEN

Only `tge pack build` needs a Telegram bot token (any bot works; it calls `getStickerSet` and `getFile`). Export it in your shell before starting the agent:

```bash
export BOT_TOKEN=...   # then start claude from this shell
```

Do not paste the token into the chat, and do not prefix commands with it: both end up in logs or shell history. `tge` reads it only from the environment, never prints or stores it; `tge doctor` shows just "set" or "not set". Searching, labeling and snippets work without a token.

## Where your data lives

Everything is in `~/.tg-premium-emoji` (override with `TG_PREMIUM_EMOJI_HOME`), outside the plugin, so plugin updates never touch it:

```
~/.tg-premium-emoji/
  catalogs/local/               your personal catalog, where new packs and labels go
  catalogs/<id>/                third-party catalogs, placed by hand (Phase 2)
  cache/                        downloaded sources and renders; safe to delete
  bin/<version>/tge             installed binaries
  bin/<version>/catalog/        the built-in catalog of that version, read-only
  bin/latest -> <version>       the active version (a directory junction on Windows)
```

A project can keep its own catalog in a `.tg-premium-emoji/` folder at its root (`tge pack build <name> --catalog project`); it takes priority over the personal one, which takes priority over the built-in one.

## Where it runs

Local agents with a persistent home folder and network access: Claude Code (CLI, desktop, IDE) and other agents on your machine, on Linux (x64, arm64), macOS (arm64, x64) and Windows (x64). The skills are plain `SKILL.md` files that other agents can read too.

claude.ai and Cowork are not supported in this version: their sandboxes keep no state between sessions and cannot download the binary. The plugin may install there, but `tge` will not run.

## Development

Requires Rust 1.89+, [Task](https://taskfile.dev) and `uv`.

```
task setup         # zig toolchain in .tools/zig, only needed without a system C++ compiler
task check         # fmt, clippy, tests
task dev-install   # local build into bin/dev, bin/latest pointed at it
claude --plugin-dir .
```

## License

MIT, see `LICENSE`.
