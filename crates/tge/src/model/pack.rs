use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub schema_version: u32,
    pub short_name: String,
    pub title: String,
    pub fetched_at: String,
    pub preview: PreviewLayout,
    #[schemars(length(max = 200))]
    pub emoji: Vec<Emoji>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PreviewLayout {
    pub cell: u32,
    pub cols: u32,
    pub per_sheet: u32,
}

impl PreviewLayout {
    pub const V1: PreviewLayout = PreviewLayout {
        cell: 144,
        cols: 5,
        per_sheet: 20,
    };
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Emoji {
    pub n: Option<u32>,
    #[schemars(regex(pattern = r"^[0-9]{1,20}$"))]
    pub id: String,
    pub file_unique_id: String,
    pub fallback: String,
    pub format: Format,
    pub repaint: bool,
    #[schemars(length(max = 64))]
    pub name: Option<String>,
    #[schemars(length(max = 300))]
    pub description: Option<String>,
    #[schemars(length(max = 24), inner(length(max = 40)))]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fits: Option<Vec<Fit>>,
    pub confidence: Option<Confidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<By>,
    pub frame_suspect: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub gone: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Static,
    Tgs,
    Webm,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum By {
    Agent,
    Human,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    Text,
    Heading,
    ListItem,
    Button,
    Status,
    Divider,
}
