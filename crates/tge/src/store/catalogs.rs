use std::fs;
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
        dirs.iter().filter_map(|d| load_pack(d)).collect()
    }
}

fn load_pack(dir: &Path) -> Option<Pack> {
    let path = dir.join("pack.json");
    if fs::metadata(&path).ok()?.len() > MAX_PACK_JSON_BYTES {
        return None;
    }
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
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

    pub fn find_pack(&self, short_name: &str) -> Option<(&Catalog, Pack)> {
        self.catalogs.iter().find_map(|c| {
            c.packs()
                .into_iter()
                .find(|p| p.short_name.eq_ignore_ascii_case(short_name))
                .map(|p| (c, p))
        })
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

fn same_dir(a: &Path, b: &Path) -> bool {
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
