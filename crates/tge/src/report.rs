//! The queue of labeling work left in a pack.

use serde::Serialize;

use crate::model::pack::{Confidence, Pack};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PackReport {
    pub pack: String,
    pub catalog: String,
    pub fetched_at: String,
    /// Present (not gone) emoji.
    pub total: usize,
    /// `n` of present emoji without a confidence.
    pub unlabeled: Vec<u32>,
    /// `n` of present emoji with confidence `low`.
    pub low: Vec<u32>,
    /// `n` of present emoji whose preview frame is suspect.
    pub frame_suspect: Vec<u32>,
    /// `id` of emoji that left the set.
    pub gone: Vec<String>,
}

pub fn pack_report(pack: &Pack, catalog_id: &str) -> PackReport {
    let present = || pack.emoji.iter().filter(|e| !e.gone);
    let numbers = |keep: &dyn Fn(&crate::model::pack::Emoji) -> bool| -> Vec<u32> {
        let mut ns: Vec<u32> = present().filter(|e| keep(e)).filter_map(|e| e.n).collect();
        ns.sort_unstable();
        ns
    };
    PackReport {
        pack: pack.short_name.clone(),
        catalog: catalog_id.to_string(),
        fetched_at: pack.fetched_at.clone(),
        total: present().count(),
        unlabeled: numbers(&|e| e.confidence.is_none()),
        low: numbers(&|e| e.confidence == Some(Confidence::Low)),
        frame_suspect: numbers(&|e| e.frame_suspect),
        gone: pack
            .emoji
            .iter()
            .filter(|e| e.gone)
            .map(|e| e.id.clone())
            .collect(),
    }
}
