use serde_json::json;

use crate::error::Error;
use crate::output::Ctx;
use crate::similar::similar_groups;
use crate::store::catalogs::Catalogs;

pub fn run(ctx: &Ctx, pack_name: &str) -> Result<(), Error> {
    let (catalogs, _) = Catalogs::discover(&ctx.env);
    let (_, pack) = catalogs.require_pack(pack_name)?;

    let mut lines = Vec::new();
    let mut groups = Vec::new();
    for (index, group) in similar_groups(&pack).iter().enumerate() {
        lines.push(format!("group {}:", index + 1));
        let mut members = Vec::new();
        for n in group {
            let Some(emoji) = pack.emoji.iter().find(|e| !e.gone && e.n == Some(*n)) else {
                continue;
            };
            let name = emoji.name.as_deref().unwrap_or_default();
            lines.push(format!("  #{n} {} {name}", emoji.fallback));
            members.push(json!({ "n": n, "name": name, "fallback": emoji.fallback }));
        }
        groups.push(members);
    }
    if lines.is_empty() {
        lines.push(format!("no similar emoji in pack {}", pack.short_name));
    }
    ctx.emit(&lines.join("\n"), json!(groups));
    Ok(())
}
