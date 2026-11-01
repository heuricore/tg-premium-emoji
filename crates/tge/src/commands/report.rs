use serde_json::json;

use crate::error::Error;
use crate::output::Ctx;
use crate::report::{PackReport, pack_report};
use crate::store::catalogs::Catalogs;

pub fn run(ctx: &Ctx, pack_name: Option<&str>) -> Result<(), Error> {
    let (catalogs, _) = Catalogs::discover(&ctx.env);
    match pack_name {
        Some(name) => {
            let (catalog, pack) = catalogs.require_pack(name)?;
            let report = pack_report(&pack, &catalog.meta.id);
            ctx.emit(&human(&report), json!(report));
        }
        None => {
            let reports: Vec<PackReport> = catalogs
                .visible_packs()
                .iter()
                .map(|(catalog, pack)| pack_report(pack, &catalog.meta.id))
                .collect();
            let text = reports.iter().map(human).collect::<Vec<_>>().join("\n");
            ctx.emit(&text, json!(reports));
        }
    }
    Ok(())
}

fn human(report: &PackReport) -> String {
    let mut lines = vec![format!(
        "pack {} ({}): {} emoji, scanned {}",
        report.pack, report.catalog, report.total, report.fetched_at
    )];
    let queues = [
        ("unlabeled", join(&report.unlabeled)),
        ("low", join(&report.low)),
        ("frame_suspect", join(&report.frame_suspect)),
        ("gone", report.gone.join(", ")),
    ];
    let before = lines.len();
    lines.extend(
        queues
            .iter()
            .filter(|(_, items)| !items.is_empty())
            .map(|(label, items)| format!("  {label}: {items}")),
    );
    if lines.len() == before {
        lines.push("  nothing left to do".to_string());
    }
    lines.join("\n")
}

fn join(ns: &[u32]) -> String {
    ns.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")
}
