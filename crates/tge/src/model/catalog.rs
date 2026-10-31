use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CatalogMeta {
    pub schema_version: u32,
    #[schemars(regex(pattern = r"^[a-z0-9][a-z0-9_-]{0,63}$"))]
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub license: String,
    pub description: String,
}
