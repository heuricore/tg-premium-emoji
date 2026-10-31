use serde_json::json;

use crate::error::Error;
use crate::output::Ctx;
use crate::store::catalogs::Catalogs;
use crate::validate::Issue;

pub fn run(ctx: &Ctx) -> Result<(), Error> {
    let (catalogs, issues) = Catalogs::discover(&ctx.env);
    let rows: Vec<_> = catalogs
        .catalogs
        .iter()
        .map(|c| {
            json!({
                "role": c.role,
                "id": c.meta.id,
                "dir": c.dir,
                "packs": c.packs().len(),
            })
        })
        .collect();
    let mut lines: Vec<String> = catalogs
        .catalogs
        .iter()
        .zip(&rows)
        .map(|(c, row)| {
            format!(
                "{} {} {} {} packs",
                c.role.label(),
                c.meta.id,
                c.dir.display(),
                row["packs"]
            )
        })
        .collect();
    lines.extend(issues.iter().map(issue_line));
    ctx.emit(
        &lines.join("\n"),
        json!({ "catalogs": rows, "issues": issues }),
    );
    Ok(())
}

pub fn issue_line(issue: &Issue) -> String {
    format!(
        "{}: {}: {}",
        issue.severity.label(),
        issue.file.display(),
        issue.message
    )
}
