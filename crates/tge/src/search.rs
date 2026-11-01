//! Tokenization and weighted per-field BM25 search over emoji metadata.

const K1: f32 = 1.2;
const B: f32 = 0.75;
const PREFIX_WEIGHT: f32 = 0.8;
const MIN_PREFIX_CHARS: usize = 4;

const W_NAME: f32 = 3.0;
const W_TAGS: f32 = 2.0;
const W_DESCRIPTION: f32 = 1.0;
const W_FITS: f32 = 0.5;

type FieldGetter = fn(&DocFields) -> &[String];

pub fn normalize(s: &str) -> String {
    s.to_lowercase().replace('ё', "е")
}

pub fn tokenize(s: &str) -> Vec<String> {
    normalize(s)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Debug, Clone, Default)]
pub struct DocFields {
    pub name: Vec<String>,
    pub tags: Vec<String>,
    pub description: Vec<String>,
    pub fits: Vec<String>,
}

#[derive(Debug)]
pub struct Bm25 {
    docs: Vec<DocFields>,
}

impl Bm25 {
    pub fn new(docs: Vec<DocFields>) -> Bm25 {
        Bm25 { docs }
    }

    /// Returns `(doc index, score)` for docs with a positive score, best first,
    /// ties broken by ascending index.
    pub fn search(&self, query: &[String]) -> Vec<(usize, f32)> {
        let mut scores = vec![0.0_f32; self.docs.len()];
        let fields: [(FieldGetter, f32); 4] = [
            (|d| &d.name, W_NAME),
            (|d| &d.tags, W_TAGS),
            (|d| &d.description, W_DESCRIPTION),
            (|d| &d.fits, W_FITS),
        ];
        for (field, weight) in fields {
            self.score_field(field, weight, query, &mut scores);
        }
        let mut hits: Vec<(usize, f32)> = scores
            .into_iter()
            .enumerate()
            .filter(|(_, score)| *score > 0.0)
            .collect();
        hits.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        hits
    }

    fn score_field(&self, field: FieldGetter, weight: f32, query: &[String], scores: &mut [f32]) {
        let total_len: usize = self.docs.iter().map(|d| field(d).len()).sum();
        if total_len == 0 {
            return;
        }
        let n = self.docs.len() as f32;
        let avg_len = total_len as f32 / n;
        for token in query {
            let tfs: Vec<f32> = self
                .docs
                .iter()
                .map(|d| term_frequency(field(d), token))
                .collect();
            let df = tfs.iter().filter(|tf| **tf > 0.0).count() as f32;
            if df == 0.0 {
                continue;
            }
            let idf = (1.0 + (n - df + 0.5) / (df + 0.5)).ln();
            for (i, tf) in tfs.iter().enumerate() {
                if *tf > 0.0 {
                    let len = field(&self.docs[i]).len() as f32;
                    let norm = tf + K1 * (1.0 - B + B * len / avg_len);
                    scores[i] += weight * idf * tf * (K1 + 1.0) / norm;
                }
            }
        }
    }
}

fn term_frequency(tokens: &[String], query: &str) -> f32 {
    let prefix_ok = query.chars().count() >= MIN_PREFIX_CHARS;
    tokens
        .iter()
        .map(|t| {
            if t == query {
                1.0
            } else if prefix_ok && t.starts_with(query) {
                PREFIX_WEIGHT
            } else {
                0.0
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(name: &str, tags: &str, description: &str) -> DocFields {
        DocFields {
            name: tokenize(name),
            tags: tokenize(tags),
            description: tokenize(description),
            fits: Vec::new(),
        }
    }

    #[test]
    fn yo_folds_to_ye() {
        assert_eq!(tokenize("Ёлка"), vec!["елка".to_string()]);
    }

    #[test]
    fn tokenize_splits_on_non_alphanumeric() {
        assert_eq!(
            tokenize("Check_mark, ok!!  done-7"),
            vec!["check", "mark", "ok", "done", "7"]
        );
    }

    #[test]
    fn prefix_matches_russian_forms() {
        let index = Bm25::new(vec![doc("other", "", ""), doc("tick", "галочка", "")]);
        let hits = index.search(&tokenize("галочк"));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, 1);
    }

    #[test]
    fn short_tokens_do_not_prefix() {
        let index = Bm25::new(vec![doc("tick", "галочка", "")]);
        assert!(index.search(&tokenize("гал")).is_empty());
    }

    #[test]
    fn name_outranks_description() {
        let index = Bm25::new(vec![doc("x", "", "star"), doc("star", "", "x")]);
        let hits = index.search(&tokenize("star"));
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].0, 1);
        assert!(hits[0].1 > hits[1].1);
    }

    #[test]
    fn tags_outrank_description() {
        let index = Bm25::new(vec![doc("x", "", "star"), doc("x", "star", "")]);
        let hits = index.search(&tokenize("star"));
        assert_eq!(hits[0].0, 1);
        assert!(hits[0].1 > hits[1].1);
    }

    #[test]
    fn no_match_returns_empty() {
        let index = Bm25::new(vec![doc("star", "sky", "bright")]);
        assert!(index.search(&tokenize("zebra")).is_empty());
    }

    #[test]
    fn exact_outranks_prefix_only() {
        let index = Bm25::new(vec![doc("x", "starlight", ""), doc("x", "star", "")]);
        let hits = index.search(&tokenize("star"));
        assert_eq!(hits[0].0, 1);
    }

    #[test]
    fn ties_break_by_index_and_empty_corpus_is_safe() {
        let index = Bm25::new(vec![doc("star", "", ""), doc("star", "", "")]);
        let hits = index.search(&tokenize("star"));
        assert_eq!(hits.iter().map(|h| h.0).collect::<Vec<_>>(), vec![0, 1]);
        assert!(Bm25::new(Vec::new()).search(&tokenize("star")).is_empty());
    }
}
