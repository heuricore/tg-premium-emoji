# tg-premium-emoji

A Claude Code plugin and a Rust CLI, `tge`, for designing Telegram bots and channels with premium (custom) emoji. Packs are rendered into images, labeled once in text, and then searched by meaning so an agent picks the right emoji instead of guessing from a fallback character.

Status: early development (v0.0.1). The design is in `docs/superpowers/specs/2026-11-01-tg-premium-emoji-design.md`; planned later work is in `ROADMAP.md`.

## Development

Requires Rust 1.89+, [Task](https://taskfile.dev) and `uv`.

```
task setup   # zig toolchain in .tools/zig, only needed without a system C++ compiler
task check   # fmt, clippy, tests
```

## License

MIT, see `LICENSE`.
