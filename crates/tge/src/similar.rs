//! Groups of look-alike emoji within one pack.

use std::collections::{BTreeMap, HashSet};

use crate::model::pack::Pack;
use crate::search::{normalize, tokenize};

const MIN_JACCARD: f64 = 0.5;

struct Entry {
    n: u32,
    name: String,
    tokens: HashSet<String>,
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    let common = a.intersection(b).count();
    let union = a.len() + b.len() - common;
    if union == 0 {
        0.0
    } else {
        common as f64 / union as f64
    }
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Present, labeled emoji grouped by similar names and tags; two are similar when their
/// normalized names are equal or the Jaccard index of their name and tag tokens is at least 0.5.
/// Groups are the connected components of size 2 or more: `n` ascending inside a group, groups
/// ordered by their smallest `n`.
pub fn similar_groups(pack: &Pack) -> Vec<Vec<u32>> {
    let mut entries: Vec<Entry> = pack
        .emoji
        .iter()
        .filter(|e| !e.gone)
        .filter_map(|e| {
            let name = e.name.as_deref()?;
            Some(Entry {
                n: e.n?,
                name: normalize(name),
                tokens: std::iter::once(name)
                    .chain(e.tags.iter().map(String::as_str))
                    .flat_map(tokenize)
                    .collect(),
            })
        })
        .collect();
    entries.sort_by_key(|e| e.n);

    let mut parent: Vec<usize> = (0..entries.len()).collect();
    for i in 0..entries.len() {
        for j in i + 1..entries.len() {
            let (a, b) = (&entries[i], &entries[j]);
            if a.name == b.name || jaccard(&a.tokens, &b.tokens) >= MIN_JACCARD {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                parent[ri.max(rj)] = ri.min(rj);
            }
        }
    }
    let mut components: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
    for (i, entry) in entries.iter().enumerate() {
        let root = find(&mut parent, i);
        components.entry(root).or_default().push(entry.n);
    }
    components.into_values().filter(|g| g.len() >= 2).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::pack::{Emoji, Format, PreviewLayout};

    fn emoji(n: u32, name: Option<&str>, tags: &[&str]) -> Emoji {
        Emoji {
            n: Some(n),
            id: format!("{n}"),
            file_unique_id: format!("U{n}"),
            fallback: "x".into(),
            format: Format::Static,
            repaint: false,
            animated: false,
            name: name.map(str::to_string),
            description: None,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            fits: None,
            confidence: name.map(|_| crate::model::pack::Confidence::High),
            by: None,
            frame_suspect: false,
            gone: false,
        }
    }

    fn pack(emoji: Vec<Emoji>) -> Pack {
        Pack {
            schema_version: 1,
            short_name: "t".into(),
            title: "T".into(),
            fetched_at: "2026-11-01T00:00:00Z".into(),
            preview: PreviewLayout::V1,
            emoji,
        }
    }

    #[test]
    fn equal_normalized_names_group() {
        let p = pack(vec![
            emoji(1, Some("Ёлка"), &[]),
            emoji(2, Some("star"), &[]),
            emoji(3, Some("елка"), &[]),
        ]);
        assert_eq!(similar_groups(&p), vec![vec![1, 3]]);
    }

    #[test]
    fn jaccard_at_half_groups_and_below_does_not() {
        // {red, heart, love} vs {red, heart, big}: 2/4 = 0.5 -> similar
        // {red, heart, love} vs {red, star, big}: 1/5 = 0.2 -> not similar
        let p = pack(vec![
            emoji(1, Some("red heart"), &["love"]),
            emoji(2, Some("red heart"), &["big"]),
            emoji(3, Some("star"), &["red", "tall"]),
        ]);
        assert_eq!(similar_groups(&p), vec![vec![1, 2]]);
        let p = pack(vec![
            emoji(1, Some("red heart"), &["love"]),
            emoji(2, Some("blue star"), &["red", "big"]),
        ]);
        assert!(similar_groups(&p).is_empty());
    }

    #[test]
    fn groups_are_connected_components_sorted_by_smallest_n() {
        let p = pack(vec![
            emoji(1, Some("dog"), &["pet"]),
            emoji(2, Some("cat"), &["pet", "meow"]),
            emoji(3, Some("sun"), &["day"]),
            emoji(4, Some("sun"), &["bright"]),
            emoji(5, Some("cat"), &["pet"]),
            emoji(6, Some("dog"), &["pet"]),
        ]);
        // 1~6 (same name), 2~5 (same name), 3~4 (same name); 1~5 via {dog,pet} vs {cat,pet} = 1/3 no
        assert_eq!(similar_groups(&p), vec![vec![1, 6], vec![2, 5], vec![3, 4]]);
        let chained = pack(vec![
            emoji(1, Some("a b"), &[]),
            emoji(2, Some("b c"), &["a"]),
            emoji(3, Some("c"), &["a", "b"]),
        ]);
        // 1~2: {a,b} vs {a,b,c} = 2/3; 2~3 = 3/3; one component
        assert_eq!(similar_groups(&chained), vec![vec![1, 2, 3]]);
    }

    #[test]
    fn unlabeled_and_gone_emoji_are_ignored() {
        let mut gone = emoji(3, Some("dog"), &[]);
        gone.gone = true;
        gone.n = None;
        let p = pack(vec![
            emoji(1, Some("dog"), &[]),
            emoji(2, None, &["dog"]),
            gone,
        ]);
        assert!(similar_groups(&p).is_empty());
    }
}
