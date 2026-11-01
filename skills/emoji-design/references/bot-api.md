# Bot API rules for premium emoji

Read this before writing the final message text or keyboard, and when the user asks where premium
emoji work. Checked against core.telegram.org/bots/api on 2026-11-01.

Take every id and fallback from `tge find` or `tge snippet` output; the forms below show the
shape, not values to fill by hand.

## Message text

**HTML** (`parse_mode: "HTML"`):

```html
<tg-emoji emoji-id="ID">FALLBACK</tg-emoji>
```

- `emoji-id` holds digits only.
- Exactly one emoji goes inside the tag: the fallback `tge` printed. Clients that cannot show the
  custom emoji show the fallback.
- Escape `<`, `>` and `&` in the rest of the text as `&lt;`, `&gt;`, `&amp;`. One stray `<` makes
  the Bot API reject the whole message.

**MarkdownV2** (`parse_mode: "MarkdownV2"`):

```
![FALLBACK](tg://emoji?id=ID)
```

- In the rest of the text, escape each of `` _ * [ ] ( ) ~ ` > # + - = | { } . ! `` with a
  preceding `\`.

Custom emoji also work in captions (photos, videos, documents) with the same parse modes, and as
a `custom_emoji` message entity carrying `custom_emoji_id` when the user builds entities instead
of using a parse mode.

## Buttons

```json
{"text": "LABEL WITHOUT EMOJI", "icon_custom_emoji_id": "ID", "style": "success"}
```

- `icon_custom_emoji_id` and `style` exist on `KeyboardButton` and `InlineKeyboardButton` since
  Bot API 9.4.
- `text` carries the label only. With an icon set, a plain emoji in `text` doubles the picture.
- `style` is one of `danger`, `success`, `primary` and colours the button. Add it only when a
  style slot sets one (Phase 2) or the user asks for a coloured button; otherwise leave it out.
- Inline buttons still need their action field (`callback_data`, `url` and so on); that part is
  the user's code.

## Where premium emoji do not work

Only plain emoji work in these places. They take plain text with no parse mode, so a `tg-emoji`
tag there would show as raw text:

- `answerCallbackQuery` notifications and alerts;
- the bot description and short description;
- the command list (`setMyCommands`);
- the menu button.

When an element of the design lands in one of these, use the fallback character or no emoji, and
say so in the hand-over table.

## Who can send them

The Bot API allows custom emoji for bots with a Fragment username, or in private chats and groups
when the bot owner has Telegram Premium.

## Checklist before handing over

- Every id came from `tge` output and is written in full.
- HTML: digits only in `emoji-id`, one emoji inside each tag, `<`, `>`, `&` escaped elsewhere.
- MarkdownV2: special characters escaped outside the emoji links.
- Buttons: a label without emoji in `text`, `style` only when asked for.
- Nothing premium in callback alerts, the bot description, the command list or the menu button.
