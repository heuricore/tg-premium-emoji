/// Escapes `&`, `<` and `>` for Telegram HTML parse mode. Quotes are left as-is.
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn tg_emoji_html(id: &str, fallback: &str) -> String {
    format!(
        r#"<tg-emoji emoji-id="{id}">{}</tg-emoji>"#,
        escape_html(fallback)
    )
}

pub fn markdown_v2(id: &str, fallback: &str) -> String {
    format!("![{}](tg://emoji?id={id})", escape_markdown_v2(fallback))
}

fn escape_markdown_v2(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if "\\_*[]()~`>#+-=|{}.!".contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

pub fn button_json(id: &str, text: &str) -> serde_json::Value {
    serde_json::json!({"text": text, "icon_custom_emoji_id": id})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tg_emoji_html_wraps_fallback_in_tag() {
        assert_eq!(
            tg_emoji_html("5368324170671202286", "✅"),
            r#"<tg-emoji emoji-id="5368324170671202286">✅</tg-emoji>"#
        );
    }

    #[test]
    fn tg_emoji_html_escapes_fallback() {
        assert_eq!(
            tg_emoji_html("5368324170671202286", "<&>"),
            r#"<tg-emoji emoji-id="5368324170671202286">&lt;&amp;&gt;</tg-emoji>"#
        );
    }

    #[test]
    fn markdown_v2_uses_emoji_link_syntax() {
        assert_eq!(
            markdown_v2("5368324170671202286", "✅"),
            "![✅](tg://emoji?id=5368324170671202286)"
        );
    }

    #[test]
    fn markdown_v2_escapes_reserved_chars_in_keycap_fallback() {
        assert_eq!(
            markdown_v2("1", "#\u{fe0f}\u{20e3}"),
            "![\\#\u{fe0f}\u{20e3}](tg://emoji?id=1)"
        );
        assert_eq!(
            markdown_v2("1", "*\u{fe0f}\u{20e3}"),
            "![\\*\u{fe0f}\u{20e3}](tg://emoji?id=1)"
        );
    }

    #[test]
    fn button_json_has_text_and_icon_keys_only() {
        assert_eq!(
            button_json("5368324170671202286", "Save"),
            serde_json::json!({"text": "Save", "icon_custom_emoji_id": "5368324170671202286"})
        );
    }

    #[test]
    fn escape_html_escapes_angle_brackets_and_ampersand() {
        assert_eq!(escape_html("a<b & c>"), "a&lt;b &amp; c&gt;");
    }

    #[test]
    fn escape_html_leaves_quotes_and_does_not_double_escape() {
        assert_eq!(escape_html(r#"say "hi" &amp;"#), r#"say "hi" &amp;amp;"#);
    }
}
