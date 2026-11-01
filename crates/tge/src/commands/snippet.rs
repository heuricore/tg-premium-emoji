use serde_json::json;

use crate::error::Error;
use crate::output::Ctx;
use crate::snippet::{button_json, markdown_v2, tg_emoji_html};
use crate::store::catalogs::Catalogs;

const MIN_ID_DIGITS: usize = 5;

pub fn run(ctx: &Ctx, pack_name: &str, target: &str, text: &str) -> Result<(), Error> {
    let catalogs = Catalogs::discover_warn(&ctx.env);
    let (_, pack) = catalogs
        .find_pack(pack_name)
        .ok_or_else(|| Error::Runtime(format!("pack '{pack_name}' not found")))?;
    let is_id = target.len() >= MIN_ID_DIGITS && target.bytes().all(|b| b.is_ascii_digit());
    let emoji = if is_id {
        pack.emoji.iter().find(|e| e.id == target)
    } else {
        let n: u32 = target.parse().map_err(|_| {
            Error::Usage(format!("'{target}' is neither an emoji number nor an id"))
        })?;
        pack.emoji.iter().find(|e| e.n == Some(n))
    }
    .ok_or_else(|| Error::Runtime(format!("emoji '{target}' not found in pack '{pack_name}'")))?;

    let html = tg_emoji_html(&emoji.id, &emoji.fallback);
    let markdown = markdown_v2(&emoji.id, &emoji.fallback);
    let button = button_json(&emoji.id, text);
    ctx.emit(
        &format!("html: {html}\nmarkdown_v2: {markdown}\nbutton: {button}"),
        json!({ "html": html, "markdown_v2": markdown, "button": button }),
    );
    Ok(())
}
