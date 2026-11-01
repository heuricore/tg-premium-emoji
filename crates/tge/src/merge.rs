use std::collections::{HashMap, HashSet};

use crate::model::pack::{By, Confidence, Emoji, Format, Pack, PreviewLayout};

/// One emoji as returned by a fresh scan; `file_id` is only used for downloading.
#[derive(Clone, Debug, PartialEq)]
pub struct FetchedEmoji {
    pub n: u32,
    pub id: String,
    pub file_unique_id: String,
    pub fallback: String,
    pub format: Format,
    pub repaint: bool,
    pub file_id: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeReport {
    pub added: Vec<String>,
    pub image_changed: Vec<String>,
    pub gone: Vec<String>,
    pub returned: Vec<String>,
    pub total: usize,
}

pub fn merge(
    existing: Option<Pack>,
    short_name: &str,
    title: &str,
    fetched: &[FetchedEmoji],
    suspect: &HashMap<String, bool>,
    now: &str,
) -> (Pack, MergeReport) {
    let old = existing.map(|p| p.emoji).unwrap_or_default();
    let mut old_by_id: HashMap<&str, &Emoji> = HashMap::new();
    for emoji in &old {
        old_by_id.entry(emoji.id.as_str()).or_insert(emoji);
    }

    let mut report = MergeReport::default();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut present: Vec<Emoji> = Vec::new();

    for f in fetched {
        if !seen.insert(f.id.as_str()) {
            continue;
        }
        let frame_suspect = suspect.get(&f.id).copied().unwrap_or(false);
        let emoji = match old_by_id.get(f.id.as_str()) {
            None => {
                report.added.push(f.id.clone());
                Emoji {
                    n: Some(f.n),
                    id: f.id.clone(),
                    file_unique_id: f.file_unique_id.clone(),
                    fallback: f.fallback.clone(),
                    format: f.format,
                    repaint: f.repaint,
                    name: None,
                    description: None,
                    tags: Vec::new(),
                    fits: None,
                    confidence: None,
                    by: None,
                    frame_suspect,
                    gone: false,
                }
            }
            Some(prev) => {
                let mut emoji = (*prev).clone();
                if prev.gone {
                    report.returned.push(f.id.clone());
                }
                if prev.file_unique_id != f.file_unique_id {
                    report.image_changed.push(f.id.clone());
                    if emoji.confidence.is_some() && emoji.by != Some(By::Human) {
                        emoji.confidence = Some(Confidence::Low);
                    }
                }
                emoji.n = Some(f.n);
                emoji.file_unique_id = f.file_unique_id.clone();
                emoji.fallback = f.fallback.clone();
                emoji.format = f.format;
                emoji.repaint = f.repaint;
                emoji.frame_suspect = frame_suspect;
                emoji.gone = false;
                emoji
            }
        };
        present.push(emoji);
    }

    // stable sort keeps fetch order for equal `n`
    present.sort_by_key(|e| e.n);
    report.total = present.len();

    let mut emoji = present;
    let mut handled: HashSet<&str> = HashSet::new();
    for prev in &old {
        if seen.contains(prev.id.as_str()) || !handled.insert(prev.id.as_str()) {
            continue;
        }
        let mut gone = prev.clone();
        if !prev.gone {
            report.gone.push(prev.id.clone());
        }
        gone.gone = true;
        gone.n = None;
        emoji.push(gone);
    }

    let pack = Pack {
        schema_version: 1,
        short_name: short_name.to_string(),
        title: title.to_string(),
        fetched_at: now.to_string(),
        preview: PreviewLayout::V1,
        emoji,
    };
    (pack, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-11-01T00:00:00Z";

    fn fetched(n: u32, id: &str, fuid: &str) -> FetchedEmoji {
        FetchedEmoji {
            n,
            id: id.into(),
            file_unique_id: fuid.into(),
            fallback: "x".into(),
            format: Format::Static,
            repaint: false,
            file_id: format!("file-{id}"),
        }
    }

    fn labeled(n: Option<u32>, id: &str, fuid: &str) -> Emoji {
        Emoji {
            n,
            id: id.into(),
            file_unique_id: fuid.into(),
            fallback: "x".into(),
            format: Format::Static,
            repaint: false,
            name: Some(format!("name-{id}")),
            description: Some(format!("desc-{id}")),
            tags: vec![format!("tag-{id}")],
            fits: None,
            confidence: Some(Confidence::High),
            by: Some(By::Agent),
            frame_suspect: false,
            gone: false,
        }
    }

    fn pack(emoji: Vec<Emoji>) -> Pack {
        Pack {
            schema_version: 1,
            short_name: "p".into(),
            title: "P".into(),
            fetched_at: "old".into(),
            preview: PreviewLayout::V1,
            emoji,
        }
    }

    fn run(existing: Option<Pack>, f: &[FetchedEmoji]) -> (Pack, MergeReport) {
        merge(existing, "p", "P", f, &HashMap::new(), NOW)
    }

    fn find<'a>(p: &'a Pack, id: &str) -> &'a Emoji {
        p.emoji.iter().find(|e| e.id == id).unwrap()
    }

    #[test]
    fn first_scan_is_unlabeled() {
        let mut sus = HashMap::new();
        sus.insert("2".to_string(), true);
        let (p, r) = merge(
            None,
            "pk",
            "Title",
            &[fetched(1, "1", "u1"), fetched(2, "2", "u2")],
            &sus,
            NOW,
        );
        assert_eq!(p.schema_version, 1);
        assert_eq!(p.short_name, "pk");
        assert_eq!(p.title, "Title");
        assert_eq!(p.fetched_at, NOW);
        assert_eq!(p.preview, PreviewLayout::V1);
        assert_eq!(p.emoji.len(), 2);
        let e = find(&p, "1");
        assert_eq!(e.n, Some(1));
        assert_eq!(e.name, None);
        assert_eq!(e.description, None);
        assert!(e.tags.is_empty());
        assert_eq!(e.fits, None);
        assert_eq!(e.confidence, None);
        assert_eq!(e.by, None);
        assert!(!e.frame_suspect);
        assert!(!e.gone);
        assert!(find(&p, "2").frame_suspect);
        assert_eq!(r.added, vec!["1", "2"]);
        assert_eq!(r.total, 2);
    }

    #[test]
    fn reorder_keeps_labels_by_id() {
        let existing = pack(vec![
            labeled(Some(1), "A", "ua"),
            labeled(Some(2), "B", "ub"),
            labeled(Some(3), "C", "uc"),
        ]);
        let (p, r) = run(
            Some(existing),
            &[
                fetched(1, "B", "ub"),
                fetched(2, "C", "uc"),
                fetched(3, "A", "ua"),
            ],
        );
        let ids: Vec<&str> = p.emoji.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["B", "C", "A"]);
        let a = find(&p, "A");
        assert_eq!(a.n, Some(3));
        assert_eq!(a.name.as_deref(), Some("name-A"));
        assert_eq!(a.description.as_deref(), Some("desc-A"));
        assert_eq!(a.tags, vec!["tag-A"]);
        assert_eq!(a.confidence, Some(Confidence::High));
        assert!(r.added.is_empty() && r.gone.is_empty() && r.image_changed.is_empty());
    }

    #[test]
    fn changed_image_lowers_confidence() {
        let existing = pack(vec![labeled(Some(1), "A", "old")]);
        let (p, r) = run(Some(existing), &[fetched(1, "A", "new")]);
        let a = find(&p, "A");
        assert_eq!(a.confidence, Some(Confidence::Low));
        assert_eq!(a.file_unique_id, "new");
        assert_eq!(a.name.as_deref(), Some("name-A"));
        assert_eq!(r.image_changed, vec!["A"]);
    }

    #[test]
    fn changed_image_keeps_human_confidence() {
        let mut e = labeled(Some(1), "A", "old");
        e.by = Some(By::Human);
        let (p, r) = run(Some(pack(vec![e])), &[fetched(1, "A", "new")]);
        let a = find(&p, "A");
        assert_eq!(a.confidence, Some(Confidence::High));
        assert_eq!(a.by, Some(By::Human));
        assert_eq!(r.image_changed, vec!["A"]);
    }

    #[test]
    fn changed_image_leaves_unlabeled_null() {
        let mut e = labeled(Some(1), "A", "old");
        e.confidence = None;
        e.by = None;
        e.name = None;
        let (p, r) = run(Some(pack(vec![e])), &[fetched(1, "A", "new")]);
        assert_eq!(find(&p, "A").confidence, None);
        assert_eq!(r.image_changed, vec!["A"]);
    }

    #[test]
    fn missing_becomes_gone() {
        let existing = pack(vec![
            labeled(Some(1), "A", "ua"),
            labeled(Some(2), "B", "ub"),
        ]);
        let (p, r) = run(Some(existing), &[fetched(1, "B", "ub")]);
        let ids: Vec<&str> = p.emoji.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["B", "A"]);
        let a = find(&p, "A");
        assert!(a.gone);
        assert_eq!(a.n, None);
        assert_eq!(a.name.as_deref(), Some("name-A"));
        assert_eq!(r.gone, vec!["A"]);
        assert_eq!(r.total, 1);
    }

    #[test]
    fn returning_loses_gone() {
        let mut g = labeled(None, "A", "ua");
        g.gone = true;
        let (p, r) = run(Some(pack(vec![g])), &[fetched(4, "A", "ua")]);
        let a = find(&p, "A");
        assert!(!a.gone);
        assert_eq!(a.n, Some(4));
        assert_eq!(a.name.as_deref(), Some("name-A"));
        assert_eq!(r.returned, vec!["A"]);
        assert!(r.image_changed.is_empty());
        assert!(r.added.is_empty());
    }

    #[test]
    fn returning_with_changed_image_is_in_both() {
        let mut g = labeled(None, "A", "old");
        g.gone = true;
        let (p, r) = run(Some(pack(vec![g])), &[fetched(1, "A", "new")]);
        let a = find(&p, "A");
        assert!(!a.gone);
        assert_eq!(a.confidence, Some(Confidence::Low));
        assert_eq!(r.returned, vec!["A"]);
        assert_eq!(r.image_changed, vec!["A"]);
    }

    #[test]
    fn already_gone_is_not_reported_again() {
        let mut g1 = labeled(None, "G1", "u");
        g1.gone = true;
        let mut g2 = labeled(None, "G2", "u");
        g2.gone = true;
        let existing = pack(vec![labeled(Some(1), "A", "ua"), g1, g2]);
        let (p, r) = run(Some(existing), &[]);
        assert_eq!(r.gone, vec!["A"]);
        let ids: Vec<&str> = p.emoji.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["A", "G1", "G2"]);
        assert!(p.emoji.iter().all(|e| e.gone && e.n.is_none()));
        assert_eq!(r.total, 0);
    }

    #[test]
    fn duplicate_fetched_ids_first_wins() {
        let (p, r) = run(
            None,
            &[
                fetched(1, "A", "first"),
                fetched(2, "A", "second"),
                fetched(3, "B", "ub"),
            ],
        );
        assert_eq!(p.emoji.len(), 2);
        assert_eq!(find(&p, "A").file_unique_id, "first");
        assert_eq!(find(&p, "A").n, Some(1));
        assert_eq!(r.added, vec!["A", "B"]);
        assert_eq!(r.total, 2);
    }

    #[test]
    fn report_counts() {
        let mut g = labeled(None, "R", "ur");
        g.gone = true;
        let existing = pack(vec![
            labeled(Some(1), "K", "uk"),
            labeled(Some(2), "C", "uc"),
            labeled(Some(3), "X", "ux"),
            g,
        ]);
        let (p, r) = run(
            Some(existing),
            &[
                fetched(1, "K", "uk"),
                fetched(2, "C", "uc2"),
                fetched(3, "N", "un"),
                fetched(4, "R", "ur"),
            ],
        );
        assert_eq!(r.added, vec!["N"]);
        assert_eq!(r.image_changed, vec!["C"]);
        assert_eq!(r.gone, vec!["X"]);
        assert_eq!(r.returned, vec!["R"]);
        assert_eq!(r.total, 4);
        assert_eq!(p.emoji.len(), 5);
    }
}
