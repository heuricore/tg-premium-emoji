use serde_json::json;

use crate::error::Error;
use crate::model::pack::{Emoji, Fit, Format};
use crate::output::Ctx;
use crate::search::{Bm25, DocFields, tokenize};
use crate::snippet::{markdown_v2, tg_emoji_html};
use crate::store::catalogs::Catalogs;

pub struct Args {
    pub query: Vec<String>,
    pub pack: Option<String>,
    pub catalog: Option<String>,
    pub fits: Option<Fit>,
    pub repaint: Option<bool>,
    pub format: Option<Format>,
    pub limit: usize,
}

struct Candidate<'a> {
    pack: &'a str,
    catalog: &'a str,
    emoji: &'a Emoji,
}

fn fit_token(fit: Fit) -> String {
    serde_json::to_value(fit)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn doc_fields(emoji: &Emoji) -> DocFields {
    DocFields {
        name: emoji.name.as_deref().map(tokenize).unwrap_or_default(),
        tags: emoji.tags.iter().flat_map(|t| tokenize(t)).collect(),
        description: emoji
            .description
            .as_deref()
            .map(tokenize)
            .unwrap_or_default(),
        fits: emoji.fits.iter().flatten().map(|f| fit_token(*f)).collect(),
    }
}

fn passes_filters(emoji: &Emoji, args: &Args) -> bool {
    args.fits
        .is_none_or(|f| emoji.fits.as_deref().is_some_and(|fits| fits.contains(&f)))
        && args.repaint.is_none_or(|r| emoji.repaint == r)
        && args.format.is_none_or(|f| emoji.format == f)
}

pub fn run(ctx: &Ctx, args: Args) -> Result<(), Error> {
    let query = tokenize(&args.query.join(" "));
    if query.is_empty() {
        return Err(Error::Usage("query has no searchable words".into()));
    }
    let (catalogs, _) = Catalogs::discover(&ctx.env);
    let packs: Vec<_> = catalogs
        .visible_packs()
        .into_iter()
        .filter(|(c, p)| {
            args.pack
                .as_deref()
                .is_none_or(|name| p.short_name.eq_ignore_ascii_case(name))
                && args.catalog.as_deref().is_none_or(|id| c.meta.id == id)
        })
        .collect();

    let mut unlabeled = Vec::new();
    let mut candidates = Vec::new();
    for (catalog, pack) in &packs {
        let count = pack
            .emoji
            .iter()
            .filter(|e| !e.gone && e.confidence.is_none())
            .count();
        if count > 0 {
            unlabeled.push((pack.short_name.as_str(), count));
        }
        for emoji in &pack.emoji {
            if !emoji.gone && emoji.confidence.is_some() && passes_filters(emoji, &args) {
                candidates.push(Candidate {
                    pack: &pack.short_name,
                    catalog: &catalog.meta.id,
                    emoji,
                });
            }
        }
    }

    let index = Bm25::new(candidates.iter().map(|c| doc_fields(c.emoji)).collect());
    let hits: Vec<_> = index
        .search(&query)
        .into_iter()
        .take(args.limit)
        .map(|(i, _)| &candidates[i])
        .collect();

    let mut lines = Vec::new();
    let mut hit_docs = Vec::new();
    for c in &hits {
        let e = c.emoji;
        let html = tg_emoji_html(&e.id, &e.fallback);
        let confidence = e.confidence.map(|v| json!(v));
        lines.push(format!(
            "{} #{} {} {} ({}) id {}\n  {html}",
            c.pack,
            e.n.unwrap_or_default(),
            e.fallback,
            e.name.as_deref().unwrap_or(""),
            confidence
                .as_ref()
                .and_then(|v| v.as_str())
                .unwrap_or_default(),
            e.id,
        ));
        hit_docs.push(json!({
            "pack": c.pack,
            "catalog": c.catalog,
            "n": e.n,
            "id": e.id,
            "name": e.name,
            "fallback": e.fallback,
            "confidence": confidence,
            "html": html,
            "markdown_v2": markdown_v2(&e.id, &e.fallback),
        }));
    }
    lines.extend(
        unlabeled
            .iter()
            .map(|(pack, count)| format!("pack {pack}: {count} unlabeled")),
    );
    let unlabeled_docs: Vec<_> = unlabeled
        .iter()
        .map(|(pack, count)| json!({ "pack": pack, "count": count }))
        .collect();
    ctx.emit(
        &lines.join("\n"),
        json!({ "hits": hit_docs, "unlabeled": unlabeled_docs }),
    );
    Ok(())
}
