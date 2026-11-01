use std::fs::File;
use std::io::{self, Read};

use serde_json::json;

use crate::error::Error;
use crate::label::{LabelOutcome, apply_labels, parse_edits};
use crate::model::pack::{By, Pack};
use crate::output::Ctx;
use crate::store::atomic::update_json;
use crate::store::catalogs::{Catalogs, Role};
use crate::validate::{Issue, MAX_PACK_JSON_BYTES, pack_issues};

/// Edits are a small JSON array; anything bigger is a mistake.
const MAX_EDITS_BYTES: u64 = 1024 * 1024;

pub fn run(ctx: &Ctx, pack_name: &str, edits: &str, by: By, force: bool) -> Result<(), Error> {
    let edits = parse_edits(&read_edits(edits)?)?;
    let (catalogs, _) = Catalogs::discover(&ctx.env);
    let (catalog, pack) = catalogs.require_pack(pack_name)?;
    if catalog.role == Role::Builtin {
        return Err(Error::Runtime(format!(
            "pack '{}' is in the built-in catalog {}, which is read-only; \
             pass its directory as --catalog-dir to edit it",
            pack.short_name,
            catalog.dir.display()
        )));
    }
    let pack_dir = catalog.pack_dir(&pack.short_name).ok_or_else(|| {
        Error::Runtime(format!(
            "no folder for pack '{}' in {}",
            pack.short_name,
            catalog.dir.display()
        ))
    })?;
    let path = pack_dir.join("pack.json");

    let mut outcome = LabelOutcome::default();
    let mut issues: Vec<Issue> = Vec::new();
    // the edits are applied to the pack as read under the lock, so a concurrent `pack build`
    // is never overwritten with stale data
    let result = update_json(&path, MAX_PACK_JSON_BYTES, |current: Option<Pack>| {
        let mut pack = current.ok_or_else(|| {
            Error::Runtime(format!("{} disappeared while editing", path.display()))
        })?;
        outcome = apply_labels(&mut pack, &edits, by, force)?;
        issues.extend(outcome.missing_confidence.iter().map(|target| {
            Issue::error(
                &path,
                format!("emoji {target} has labels but no confidence; state high, medium or low"),
            )
        }));
        issues.extend(pack_issues(&pack, &path));
        if issues.is_empty() {
            Ok(pack)
        } else {
            Err(Error::Invalid(issues.len()))
        }
    });
    if let Err(err) = result {
        if matches!(err, Error::Invalid(_)) {
            report_issues(ctx, &issues);
        }
        return Err(err);
    }

    ctx.emit(
        &human(&outcome),
        json!({
            "pack": pack.short_name,
            "applied": outcome.applied,
            "skipped_human": outcome.skipped_human,
            "not_found": outcome.not_found,
        }),
    );
    Ok(())
}

fn read_edits(source: &str) -> Result<String, Error> {
    let mut text = String::new();
    let read = if source == "-" {
        io::stdin()
            .take(MAX_EDITS_BYTES + 1)
            .read_to_string(&mut text)
    } else {
        File::open(source)
            .map_err(|e| Error::Usage(format!("cannot read {source}: {e}")))?
            .take(MAX_EDITS_BYTES + 1)
            .read_to_string(&mut text)
    };
    read.map_err(|e| Error::Usage(format!("cannot read edits from {source}: {e}")))?;
    if text.len() as u64 > MAX_EDITS_BYTES {
        return Err(Error::Usage(format!(
            "edits are larger than {MAX_EDITS_BYTES} bytes"
        )));
    }
    Ok(text)
}

fn report_issues(ctx: &Ctx, issues: &[Issue]) {
    if ctx.json {
        println!("{}", json!({ "issues": issues, "errors": issues.len() }));
    } else {
        for issue in issues {
            eprintln!(
                "{}: {}: {}",
                issue.severity.label(),
                issue.file.display(),
                issue.message
            );
        }
    }
}

fn human(outcome: &LabelOutcome) -> String {
    let mut lines = vec![format!(
        "applied {}: {}",
        outcome.applied.len(),
        outcome.applied.join(", ")
    )];
    if !outcome.skipped_human.is_empty() {
        lines.push(format!(
            "skipped, labeled by a human (use --force): {}",
            outcome.skipped_human.join(", ")
        ));
    }
    if !outcome.not_found.is_empty() {
        lines.push(format!("not found: {}", outcome.not_found.join(", ")));
    }
    lines.join("\n")
}
