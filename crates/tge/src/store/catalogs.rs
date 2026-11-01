use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::error::Error;
use crate::model::SCHEMA_VERSION;
use crate::model::catalog::CatalogMeta;
use crate::model::pack::Pack;
use crate::paths::{Env, PROJECT_DIR, find_project_catalog};
use crate::schema::DocKind;
use crate::validate::{Issue, MAX_PACK_JSON_BYTES, validate_value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Explicit,
    Project,
    Local,
    Builtin,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::Explicit => "explicit",
            Role::Project => "project",
            Role::Local => "local",
            Role::Builtin => "builtin",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub role: Role,
    pub dir: PathBuf,
    pub meta: CatalogMeta,
}

impl Catalog {
    /// Packs whose `pack.json` loads and parses, sorted by directory name.
    pub fn packs(&self) -> Vec<Pack> {
        let Ok(entries) = fs::read_dir(self.dir.join("packs")) else {
            return Vec::new();
        };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        dirs.iter()
            .filter_map(|d| load_pack(d).ok().flatten())
            .collect()
    }

    /// Problems (path and reason) of pack folders whose `pack.json` exists but does not load.
    pub fn pack_problems(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.dir.join("packs")) else {
            return Vec::new();
        };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        dirs.iter().filter_map(|d| load_pack(d).err()).collect()
    }

    /// The folder of the pack `short_name` under this catalog, matched ignoring ASCII case.
    pub fn pack_dir(&self, short_name: &str) -> Option<PathBuf> {
        let packs = self.dir.join("packs");
        let folder = pack_folder(&packs, short_name).ok()??;
        Some(packs.join(folder))
    }
}

/// The folder in `packs` whose name matches `name` ignoring ASCII case (an exact match first).
/// `Ok(None)` when there is none or `packs` does not exist.
pub fn pack_folder(packs: &Path, name: &str) -> io::Result<Option<String>> {
    let entries = match fs::read_dir(packs) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let mut matches: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|folder| folder.eq_ignore_ascii_case(name))
        .collect();
    matches.sort();
    if matches.iter().any(|m| m == name) {
        return Ok(Some(name.to_string()));
    }
    Ok(matches.into_iter().next())
}

/// `Ok(None)` when the folder has no `pack.json`; `Err` is a "path: reason" message.
fn load_pack(dir: &Path) -> Result<Option<Pack>, String> {
    let path = dir.join("pack.json");
    let fail = |reason: String| format!("{}: {reason}", path.display());
    let meta = match fs::metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(fail(e.to_string())),
    };
    if meta.len() > MAX_PACK_JSON_BYTES {
        return Err(fail(format!("larger than {MAX_PACK_JSON_BYTES} bytes")));
    }
    let text = fs::read_to_string(&path).map_err(|e| fail(e.to_string()))?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| fail(format!("invalid pack.json: {e}")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteTarget {
    Local,
    Project,
    Explicit,
}

#[derive(Debug, Default)]
pub struct Catalogs {
    /// Highest priority first.
    pub catalogs: Vec<Catalog>,
}

impl Catalogs {
    pub fn discover(env: &Env) -> (Catalogs, Vec<Issue>) {
        let mut candidates: Vec<(Role, PathBuf, bool)> = Vec::new();
        if let Some(dir) = &env.catalog_dir {
            candidates.push((Role::Explicit, dir.clone(), true));
        }
        if let Some(dir) = find_project_catalog(&env.cwd) {
            candidates.push((Role::Project, dir, true));
        }
        candidates.push((Role::Local, local_dir(env), false));
        if let Some(dir) = &env.builtin {
            candidates.push((Role::Builtin, dir.clone(), true));
        }

        let mut catalogs = Vec::new();
        let mut issues = Vec::new();
        for (role, dir, required) in candidates {
            match load_meta(&dir, required) {
                Ok(Some(meta)) => catalogs.push(Catalog { role, dir, meta }),
                Ok(None) => {}
                Err(issue) => issues.extend(issue),
            }
        }
        (Catalogs { catalogs }, issues)
    }

    /// `discover`, with every discovery issue and unloadable pack reported as a stderr warning.
    pub fn discover_warn(env: &Env) -> Catalogs {
        let (catalogs, issues) = Self::discover(env);
        for issue in issues {
            eprintln!("warning: {}: {}", issue.file.display(), issue.message);
        }
        for catalog in &catalogs.catalogs {
            for problem in catalog.pack_problems() {
                eprintln!("warning: {problem}");
            }
        }
        catalogs
    }

    pub fn find_pack(&self, short_name: &str) -> Option<(&Catalog, Pack)> {
        self.catalogs.iter().find_map(|c| {
            c.packs()
                .into_iter()
                .find(|p| p.short_name.eq_ignore_ascii_case(short_name))
                .map(|p| (c, p))
        })
    }

    /// `find_pack`, with the "not found" runtime error commands report.
    pub fn require_pack(&self, short_name: &str) -> Result<(&Catalog, Pack), Error> {
        self.find_pack(short_name)
            .ok_or_else(|| Error::Runtime(format!("pack '{short_name}' not found")))
    }

    pub fn visible_packs(&self) -> Vec<(&Catalog, Pack)> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for catalog in &self.catalogs {
            for pack in catalog.packs() {
                if seen.insert(pack.short_name.to_ascii_lowercase()) {
                    out.push((catalog, pack));
                }
            }
        }
        out
    }

    /// Directory a write command would modify, resolved without creating anything. Never the
    /// built-in catalog.
    pub fn target_dir(env: &Env, target: WriteTarget) -> Result<PathBuf, Error> {
        let dir = match target {
            WriteTarget::Local => local_dir(env),
            WriteTarget::Project => {
                find_project_catalog(&env.cwd).unwrap_or_else(|| env.cwd.join(PROJECT_DIR))
            }
            WriteTarget::Explicit => match &env.catalog_dir {
                Some(dir) => dir.clone(),
                None => return Err(Error::Usage("--catalog-dir is required".into())),
            },
        };
        if let Some(builtin) = &env.builtin
            && same_dir(builtin, &dir)
        {
            return Err(Error::Runtime(format!(
                "the built-in catalog {} is read-only",
                builtin.display()
            )));
        }
        Ok(dir)
    }

    /// `target_dir`, created with a generated `catalog.json` when missing.
    pub fn writable_dir(env: &Env, target: WriteTarget) -> Result<PathBuf, Error> {
        let dir = Self::target_dir(env, target)?;
        let id = match target {
            WriteTarget::Local => "local",
            WriteTarget::Project => "project",
            WriteTarget::Explicit => "custom",
        };
        let catalog_path = dir.join("catalog.json");
        if !catalog_path.exists() {
            fs::create_dir_all(&dir)?;
            let meta = CatalogMeta {
                schema_version: SCHEMA_VERSION,
                id: id.to_string(),
                name: id.to_string(),
                version: "0.1.0".to_string(),
                author: String::new(),
                license: String::new(),
                description: format!("{id} catalog"),
            };
            fs::write(&catalog_path, serde_json::to_string_pretty(&meta)? + "\n")?;
        }
        Ok(dir)
    }
}

fn local_dir(env: &Env) -> PathBuf {
    env.home.join("catalogs").join("local")
}

pub fn same_dir(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// `Ok(None)` for an optional catalog that simply does not exist.
fn load_meta(dir: &Path, required: bool) -> Result<Option<CatalogMeta>, Vec<Issue>> {
    let path = dir.join("catalog.json");
    if !required && !dir.exists() {
        return Ok(None);
    }
    let fail = |message: String| Err(vec![Issue::error(&path, message)]);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => return fail(format!("cannot read catalog: {e}")),
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return fail(format!("invalid JSON: {e}")),
    };
    let messages = validate_value(DocKind::Catalog, &value);
    if !messages.is_empty() {
        return Err(messages
            .into_iter()
            .map(|m| Issue::error(&path, m))
            .collect());
    }
    match serde_json::from_value(value) {
        Ok(meta) => Ok(Some(meta)),
        Err(e) => fail(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_folder_ignores_case_and_prefers_exact() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Demo")).unwrap();
        assert_eq!(
            pack_folder(dir.path(), "demo").unwrap().as_deref(),
            Some("Demo")
        );
        assert_eq!(pack_folder(dir.path(), "other").unwrap(), None);
        fs::create_dir_all(dir.path().join("demo")).unwrap();
        assert_eq!(
            pack_folder(dir.path(), "demo").unwrap().as_deref(),
            Some("demo")
        );
        assert_eq!(pack_folder(&dir.path().join("none"), "demo").unwrap(), None);
    }
}
