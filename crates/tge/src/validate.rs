use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::model::SCHEMA_VERSION;
use crate::model::pack::Pack;
use crate::schema::{self, DocKind};

pub const MAX_PACK_JSON_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_PNG_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub severity: Severity,
    pub file: PathBuf,
    pub message: String,
}

impl Issue {
    pub(crate) fn error(file: &Path, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            file: file.to_path_buf(),
            message: message.into(),
        }
    }

    fn warning(file: &Path, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            file: file.to_path_buf(),
            message: message.into(),
        }
    }
}

/// Checks `value` against the generated JSON Schema and the pinned `schema_version`.
pub fn validate_value(kind: DocKind, value: &Value) -> Vec<String> {
    let schema = schema::generate(kind);
    let validator = jsonschema::validator_for(&schema).expect("generated schema compiles");
    let mut messages: Vec<String> = validator
        .iter_errors(value)
        .map(|e| format!("{}: {}", e.instance_path(), e))
        .collect();
    if let Some(version) = value.get("schema_version").and_then(Value::as_u64)
        && version != u64::from(SCHEMA_VERSION)
    {
        messages.push(format!(
            "/schema_version: unsupported version {version}, expected {SCHEMA_VERSION}"
        ));
    }
    messages
}

/// Schema and semantic errors of an in-memory pack (no preview-sheet check). Semantics run only
/// when the schema is clean.
pub fn pack_issues(pack: &Pack, file: &Path) -> Vec<Issue> {
    let mut issues = Vec::new();
    match serde_json::to_value(pack) {
        Ok(value) => {
            if schema_issues(DocKind::Pack, &value, file, &mut issues) {
                check_pack_semantics(pack, file, &mut issues);
            }
        }
        Err(e) => issues.push(Issue::error(file, e.to_string())),
    }
    issues
}

pub fn validate_catalog_dir(dir: &Path) -> Vec<Issue> {
    let mut issues = Vec::new();

    let catalog_path = dir.join("catalog.json");
    if let Some(value) = read_json(&catalog_path, None, &mut issues) {
        schema_issues(DocKind::Catalog, &value, &catalog_path, &mut issues);
    }

    let packs_dir = dir.join("packs");
    let mut pack_dirs: Vec<PathBuf> = match fs::read_dir(&packs_dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect(),
        Err(e) => {
            issues.push(Issue::error(&packs_dir, format!("cannot read packs: {e}")));
            Vec::new()
        }
    };
    pack_dirs.sort();
    for pack_dir in pack_dirs {
        validate_pack_dir(&pack_dir, &mut issues);
    }
    issues
}

fn schema_issues(kind: DocKind, value: &Value, file: &Path, issues: &mut Vec<Issue>) -> bool {
    let messages = validate_value(kind, value);
    let clean = messages.is_empty();
    issues.extend(messages.into_iter().map(|m| Issue::error(file, m)));
    clean
}

fn read_json(path: &Path, max_bytes: Option<u64>, issues: &mut Vec<Issue>) -> Option<Value> {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            issues.push(Issue::error(path, format!("cannot read file: {e}")));
            return None;
        }
    };
    if let Some(max) = max_bytes
        && meta.len() > max
    {
        issues.push(Issue::error(
            path,
            format!("file is {} bytes, limit is {max}", meta.len()),
        ));
        return None;
    }
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            issues.push(Issue::error(path, format!("cannot read file: {e}")));
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            issues.push(Issue::error(path, format!("invalid JSON: {e}")));
            None
        }
    }
}

fn validate_pack_dir(pack_dir: &Path, issues: &mut Vec<Issue>) {
    let pack_path = pack_dir.join("pack.json");
    let Some(value) = read_json(&pack_path, Some(MAX_PACK_JSON_BYTES), issues) else {
        return;
    };
    if !schema_issues(DocKind::Pack, &value, &pack_path, issues) {
        return;
    }
    let pack: Pack = match serde_json::from_value(value) {
        Ok(p) => p,
        Err(e) => {
            issues.push(Issue::error(&pack_path, e.to_string()));
            return;
        }
    };
    check_pack_semantics(&pack, &pack_path, issues);
    check_sheets(&pack, pack_dir, issues);
    if pack_dir
        .file_name()
        .is_some_and(|n| n != pack.short_name.as_str())
    {
        issues.push(Issue::warning(
            &pack_path,
            format!(
                "directory name differs from short_name '{}'",
                pack.short_name
            ),
        ));
    }
}

fn check_pack_semantics(pack: &Pack, file: &Path, issues: &mut Vec<Issue>) {
    let mut ids = HashSet::new();
    let mut numbers = HashSet::new();
    for emoji in &pack.emoji {
        if !ids.insert(emoji.id.as_str()) {
            issues.push(Issue::error(file, format!("duplicate id {}", emoji.id)));
        }
        match (emoji.gone, emoji.n) {
            (true, Some(n)) => issues.push(Issue::error(
                file,
                format!("emoji {} is gone but has n = {n}", emoji.id),
            )),
            (false, None) => issues.push(Issue::error(
                file,
                format!("emoji {} is not gone but has n = null", emoji.id),
            )),
            (false, Some(n)) => {
                if !numbers.insert(n) {
                    issues.push(Issue::error(file, format!("duplicate n {n}")));
                }
            }
            (true, None) => {}
        }
    }
    let live = pack.emoji.iter().filter(|e| !e.gone).count();
    let contiguous = (1..=live as u32).all(|n| numbers.contains(&n));
    if !contiguous || numbers.len() > live {
        issues.push(Issue::error(
            file,
            format!("n values must be 1..={live} without gaps"),
        ));
    }
}

fn check_sheets(pack: &Pack, pack_dir: &Path, issues: &mut Vec<Issue>) {
    let preview_dir = pack_dir.join("preview");
    if pack.preview.per_sheet == 0 {
        issues.push(Issue::error(
            &pack_dir.join("pack.json"),
            "preview.per_sheet must be positive",
        ));
        return;
    }
    if pack.short_name.contains(['/', '\\']) || pack.short_name.contains("..") {
        issues.push(Issue::error(
            &pack_dir.join("pack.json"),
            "short_name must not contain path separators",
        ));
        return;
    }
    let live = pack.emoji.iter().filter(|e| !e.gone).count() as u32;
    for page in 1..=live.div_ceil(pack.preview.per_sheet) {
        let sheet = preview_dir.join(format!("{}_{page}.png", pack.short_name));
        match fs::metadata(&sheet) {
            Err(_) => issues.push(Issue::error(&sheet, "preview sheet is missing")),
            Ok(m) if m.len() > MAX_PNG_BYTES => issues.push(Issue::error(
                &sheet,
                format!("file is {} bytes, limit is {MAX_PNG_BYTES}", m.len()),
            )),
            Ok(_) => {}
        }
    }
}
