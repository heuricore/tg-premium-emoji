//! Label edits: parsing a batch and applying it to a pack.

use serde::{Deserialize, Deserializer};

use crate::error::Error;
use crate::model::pack::{By, Confidence, Fit, Pack};

/// One edit. For `name`, `description`, `fits` and `confidence`: absent keeps the field, `null`
/// clears it, a value sets it. `tags`: absent keeps, an array replaces.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LabelEdit {
    pub n: Option<u32>,
    pub id: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub name: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub tags: Option<Vec<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub fits: Option<Option<Vec<Fit>>>,
    #[serde(default, deserialize_with = "nullable")]
    pub confidence: Option<Option<Confidence>>,
}

fn nullable<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// A field that may be absent but not `null`.
fn present<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LabelOutcome {
    /// Targets (the `n` or `id` as written in the edit) that were changed.
    pub applied: Vec<String>,
    pub skipped_human: Vec<String>,
    pub not_found: Vec<String>,
    /// Applied targets left with labels but no confidence.
    pub missing_confidence: Vec<String>,
}

enum Target<'a> {
    N(u32),
    Id(&'a str),
}

impl LabelEdit {
    fn target(&self) -> Result<Target<'_>, Error> {
        match (self.n, self.id.as_deref()) {
            (Some(n), None) => Ok(Target::N(n)),
            (None, Some(id)) => Ok(Target::Id(id)),
            _ => Err(Error::Usage(
                "each edit needs exactly one of \"n\" and \"id\"".into(),
            )),
        }
    }
}

/// Parses the JSON array of edits; every edit names exactly one of `n` and `id`.
pub fn parse_edits(text: &str) -> Result<Vec<LabelEdit>, Error> {
    let edits: Vec<LabelEdit> = serde_json::from_str(text)
        .map_err(|e| Error::Usage(format!("invalid label edits: {e}")))?;
    for edit in &edits {
        edit.target()?;
        if edit.name.is_none()
            && edit.description.is_none()
            && edit.tags.is_none()
            && edit.fits.is_none()
            && edit.confidence.is_none()
        {
            return Err(Error::Usage(
                "each edit needs at least one field besides \"n\" or \"id\"".into(),
            ));
        }
    }
    Ok(edits)
}

/// Applies `edits` in order. Edits by `n` match present emoji only, edits by `id` also gone ones.
/// Every applied emoji gets `by`; an agent edit on a `by: human` emoji is skipped unless `force`.
pub fn apply_labels(
    pack: &mut Pack,
    edits: &[LabelEdit],
    by: By,
    force: bool,
) -> Result<LabelOutcome, Error> {
    let mut outcome = LabelOutcome::default();
    for edit in edits {
        let target = edit.target()?;
        let label = match target {
            Target::N(n) => n.to_string(),
            Target::Id(id) => id.to_string(),
        };
        let found = pack.emoji.iter_mut().find(|e| match target {
            Target::N(n) => !e.gone && e.n == Some(n),
            Target::Id(id) => e.id == id,
        });
        let Some(emoji) = found else {
            outcome.not_found.push(label);
            continue;
        };
        if by == By::Agent && emoji.by == Some(By::Human) && !force {
            outcome.skipped_human.push(label);
            continue;
        }
        if let Some(name) = &edit.name {
            emoji.name.clone_from(name);
        }
        if let Some(description) = &edit.description {
            emoji.description.clone_from(description);
        }
        if let Some(tags) = &edit.tags {
            emoji.tags.clone_from(tags);
        }
        if let Some(fits) = &edit.fits {
            emoji.fits.clone_from(fits);
        }
        if let Some(confidence) = edit.confidence {
            emoji.confidence = confidence;
        }
        emoji.by = Some(by);
        let labeled = emoji.name.is_some()
            || emoji.description.is_some()
            || !emoji.tags.is_empty()
            || emoji.fits.is_some();
        if labeled && emoji.confidence.is_none() {
            outcome.missing_confidence.push(label.clone());
        }
        outcome.applied.push(label);
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tri_state_fields_parse() {
        let edits = parse_edits(
            r#"[{"n":1,"name":null,"description":"d"},{"id":"7","tags":[],"confidence":null}]"#,
        )
        .unwrap();
        assert_eq!(edits[0].name, Some(None));
        assert_eq!(edits[0].description, Some(Some("d".to_string())));
        assert_eq!(edits[0].fits, None);
        assert_eq!(edits[1].tags, Some(vec![]));
        assert_eq!(edits[1].confidence, Some(None));
    }

    #[test]
    fn target_must_be_exactly_one() {
        assert!(parse_edits(r#"[{"name":"x"}]"#).is_err());
        assert!(parse_edits(r#"[{"n":1,"id":"1"}]"#).is_err());
        assert!(parse_edits(r#"[{"n":1,"tags":null}]"#).is_err());
        assert!(parse_edits(r#"[{"n":1,"bogus":1}]"#).is_err());
    }
}
