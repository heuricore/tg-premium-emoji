use std::path;

use serde_json::{Value, json};

use crate::commands::snippet::resolve;
use crate::error::Error;
use crate::model::pack::Emoji;
use crate::output::Ctx;
use crate::render::sheet::{COLS, PER_SHEET, page_of, sheet_file_name};
use crate::snippet::{button_json, markdown_v2, tg_emoji_html};
use crate::store::catalogs::Catalogs;

/// Preview position of a present emoji, rows and columns counted from 1.
fn cell_of(n: u32) -> (u32, u32, u32) {
    let pos = n.saturating_sub(1) % PER_SHEET;
    (page_of(n), pos / COLS + 1, pos % COLS + 1)
}

fn token<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn text(value: &Value) -> String {
    match value {
        Value::Null => "-".into(),
        Value::String(s) if s.is_empty() => "-".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn record(emoji: &Emoji, catalog: Value, pack: &crate::model::pack::Pack, sheet: Value) -> Value {
    let (page, row, column) = match emoji.n {
        Some(n) => {
            let (p, r, c) = cell_of(n);
            (json!(p), json!(r), json!(c))
        }
        None => (Value::Null, Value::Null, Value::Null),
    };
    let sheet = if emoji.n.is_some() {
        sheet
    } else {
        Value::Null
    };
    json!({
        "n": emoji.n,
        "id": emoji.id,
        "file_unique_id": emoji.file_unique_id,
        "fallback": emoji.fallback,
        "format": token(&emoji.format),
        "animated": emoji.animated,
        "repaint": emoji.repaint,
        "name": emoji.name,
        "description": emoji.description,
        "tags": emoji.tags,
        "fits": emoji.fits,
        "confidence": emoji.confidence.map(|c| token(&c)),
        "by": emoji.by.map(|b| token(&b)),
        "frame_suspect": emoji.frame_suspect,
        "gone": emoji.gone,
        "catalog": catalog["id"],
        "catalog_role": catalog["role"],
        "pack": pack.short_name,
        "pack_title": pack.title,
        "sheet": sheet,
        "page": page,
        "row": row,
        "column": column,
        "html": tg_emoji_html(&emoji.id, &emoji.fallback),
        "markdown_v2": markdown_v2(&emoji.id, &emoji.fallback),
        "button": button_json(&emoji.id, "LABEL"),
    })
}

const KEYS: [&str; 26] = [
    "n",
    "id",
    "file_unique_id",
    "fallback",
    "format",
    "animated",
    "repaint",
    "name",
    "description",
    "tags",
    "fits",
    "confidence",
    "by",
    "frame_suspect",
    "gone",
    "catalog",
    "catalog_role",
    "pack",
    "pack_title",
    "sheet",
    "page",
    "row",
    "column",
    "html",
    "markdown_v2",
    "button",
];

fn block(doc: &Value) -> String {
    KEYS.iter()
        .map(|key| {
            let value = &doc[*key];
            let shown = match value {
                Value::Array(items) => {
                    if items.is_empty() {
                        "-".to_string()
                    } else {
                        items.iter().map(text).collect::<Vec<_>>().join(", ")
                    }
                }
                other => text(other),
            };
            format!("{key}: {shown}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn run(ctx: &Ctx, pack_name: &str, targets: &[String]) -> Result<(), Error> {
    let catalogs = Catalogs::discover_warn(&ctx.env);
    let (catalog, pack) = catalogs.require_pack(pack_name)?;
    let emojis = targets
        .iter()
        .map(|t| resolve(&pack, pack_name, t))
        .collect::<Result<Vec<_>, _>>()?;

    let pack_dir = catalog.pack_dir(&pack.short_name).ok_or_else(|| {
        Error::Runtime(format!(
            "no folder for pack '{}' in {}",
            pack.short_name,
            catalog.dir.display()
        ))
    })?;
    let preview = path::absolute(pack_dir.join("preview"))?;
    let catalog_doc = json!({ "id": catalog.meta.id, "role": catalog.role.label() });

    let docs: Vec<Value> = emojis
        .iter()
        .map(|e| {
            let sheet = e.n.map_or(Value::Null, |n| {
                json!(
                    preview
                        .join(sheet_file_name(&pack.short_name, page_of(n)))
                        .display()
                        .to_string()
                )
            });
            record(e, catalog_doc.clone(), &pack, sheet)
        })
        .collect();
    let human = docs.iter().map(block).collect::<Vec<_>>().join("\n\n");
    ctx.emit(&human, Value::Array(docs));
    Ok(())
}
