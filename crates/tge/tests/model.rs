use tge::model::pack::{Emoji, Format};
use tge::schema::DocKind;

fn sample_emoji() -> Emoji {
    Emoji {
        n: Some(1),
        id: "5368324170671202286".into(),
        file_unique_id: "AgADabc".into(),
        fallback: "\u{1F600}".into(),
        format: Format::Static,
        repaint: false,
        animated: false,
        name: None,
        description: None,
        tags: vec![],
        fits: None,
        confidence: None,
        by: None,
        frame_suspect: false,
        gone: false,
    }
}

#[test]
fn emoji_roundtrip_omits_defaults() {
    let e = sample_emoji();
    let v = serde_json::to_value(&e).unwrap();
    assert!(v.get("gone").is_none() && v.get("by").is_none() && v.get("fits").is_none());
    assert!(v["name"].is_null() && v["confidence"].is_null());
    assert_eq!(serde_json::from_value::<Emoji>(v).unwrap(), e);
}

#[test]
fn unknown_field_rejected() {
    let mut v = serde_json::to_value(sample_emoji()).unwrap();
    v["extra"] = 1.into();
    assert!(serde_json::from_value::<Emoji>(v).is_err());
}

#[test]
fn schema_constraints_present() {
    let s = tge::schema::generate(DocKind::Pack).to_string();
    for needle in [
        r#""additionalProperties":false"#,
        r#""^[0-9]{1,20}$""#,
        r#""maxLength":300"#,
        r#""maxLength":64"#,
        r#""maxLength":40"#,
        r#""maxItems":24"#,
    ] {
        assert!(s.contains(needle), "missing {needle}");
    }
    assert_eq!(
        s.matches("maxItems").count(),
        1,
        "emoji list must be unbounded"
    );
    let c = tge::schema::generate(DocKind::Catalog).to_string();
    assert!(c.contains(r#""^[a-z0-9][a-z0-9_-]{0,63}$""#));
}

#[test]
fn committed_schemas_match_generated() {
    for (kind, file) in [(DocKind::Catalog, "catalog"), (DocKind::Pack, "pack")] {
        let path = format!(
            "{}/../../schemas/{file}.schema.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let committed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(
            tge::schema::generate(kind),
            committed,
            "{file} schema drifted; run task schema"
        );
    }
}
