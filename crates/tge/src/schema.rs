use std::path::Path;

use schemars::{JsonSchema, generate::SchemaSettings};
use serde_json::Value;

use crate::error::Error;
use crate::model::catalog::CatalogMeta;
use crate::model::pack::Pack;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocKind {
    Catalog,
    Pack,
}

impl DocKind {
    pub const ALL: [DocKind; 2] = [DocKind::Catalog, DocKind::Pack];

    pub fn file_stem(self) -> &'static str {
        match self {
            DocKind::Catalog => "catalog",
            DocKind::Pack => "pack",
        }
    }
}

fn root<T: JsonSchema>() -> Value {
    let generator = SchemaSettings::draft2020_12().into_generator();
    serde_json::to_value(generator.into_root_schema_for::<T>()).expect("schema is valid JSON")
}

pub fn generate(kind: DocKind) -> Value {
    let mut schema = match kind {
        DocKind::Catalog => root::<CatalogMeta>(),
        DocKind::Pack => root::<Pack>(),
    };
    schema["$id"] = format!(
        "https://raw.githubusercontent.com/heuricore/tg-premium-emoji/main/schemas/{}.schema.json",
        kind.file_stem()
    )
    .into();
    schema
}

pub fn write_all(out: &Path) -> Result<(), Error> {
    std::fs::create_dir_all(out)?;
    for kind in DocKind::ALL {
        let mut text = serde_json::to_string_pretty(&generate(kind))?;
        text.push('\n');
        std::fs::write(out.join(format!("{}.schema.json", kind.file_stem())), text)?;
    }
    Ok(())
}
