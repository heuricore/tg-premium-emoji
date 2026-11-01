use std::path::{self, PathBuf};

use serde_json::json;

use crate::error::Error;
use crate::output::Ctx;
use crate::render::sheet::{PER_SHEET, sheet_count, sheet_file_name};
use crate::store::catalogs::Catalogs;

struct Sheet {
    page: u32,
    path: PathBuf,
    from: u32,
    to: u32,
}

pub fn run(ctx: &Ctx, short_name: &str) -> Result<(), Error> {
    let catalogs = Catalogs::discover_warn(&ctx.env);
    let (catalog, pack) = catalogs.require_pack(short_name)?;
    let pack_dir = catalog.pack_dir(&pack.short_name).ok_or_else(|| {
        Error::Runtime(format!(
            "no folder for pack '{}' in {}",
            pack.short_name,
            catalog.dir.display()
        ))
    })?;
    let preview = path::absolute(pack_dir.join("preview"))?;

    let highest = pack
        .emoji
        .iter()
        .filter(|e| !e.gone)
        .filter_map(|e| e.n)
        .max()
        .unwrap_or(0);
    let sheets: Vec<Sheet> = (1..=sheet_count(highest))
        .map(|page| Sheet {
            page,
            path: preview.join(sheet_file_name(&pack.short_name, page)),
            from: (page - 1) * PER_SHEET + 1,
            to: (page * PER_SHEET).min(highest),
        })
        .collect();

    let missing: Vec<String> = sheets
        .iter()
        .filter(|s| !s.path.is_file())
        .map(|s| s.path.display().to_string())
        .collect();
    if !missing.is_empty() {
        return Err(Error::Runtime(format!(
            "missing preview sheets: {}; run `tge pack build {}` to render them",
            missing.join(", "),
            pack.short_name
        )));
    }

    let human = if sheets.is_empty() {
        format!("pack {} has no emoji", pack.short_name)
    } else {
        sheets
            .iter()
            .map(|s| format!("{}  {}-{}", s.path.display(), s.from, s.to))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let docs: Vec<_> = sheets
        .iter()
        .map(|s| json!({ "page": s.page, "path": s.path, "from": s.from, "to": s.to }))
        .collect();
    ctx.emit(&human, json!({ "pack": pack.short_name, "sheets": docs }));
    Ok(())
}
