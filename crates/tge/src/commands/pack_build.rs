//! `tge pack build`: download a custom emoji set, render its preview sheets, write `pack.json`.
//!
//! Phases run in order so a failure before phase C leaves the catalog untouched: A downloads
//! every source into the cache, B renders one icon per emoji in memory, C merges, swaps in the
//! new preview folder and writes `pack.json`, all under the pack's lock.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::botapi::BotApi;
use crate::cli::CatalogChoice;
use crate::error::Error;
use crate::merge::{FetchedEmoji, MergeReport, merge};
use crate::model::pack::{Format, Pack};
use crate::output::Ctx;
use crate::paths::Env;
use crate::render::frames::{
    FrameStats, SAMPLES, coverage, is_frame_suspect, pick_best, sample_positions, whiten,
};
use crate::render::sheet::{ICON, compose, page_of, sheet_file_name, write_png};
use crate::render::{Media, Rgba};
use crate::store::atomic::{update_json, write_atomic};
use crate::store::catalogs::{Catalogs, WriteTarget, pack_folder};
use crate::validate::MAX_PACK_JSON_BYTES;

const PREVIEW: &str = "preview";
/// Prefix of the staging folders next to `preview/`.
const STAGING: &str = ".preview-";

pub fn run(ctx: &Ctx, short_name: &str, catalog: Option<CatalogChoice>) -> Result<(), Error> {
    if !is_set_name(short_name) {
        return Err(Error::Usage(format!(
            "'{short_name}' is not a set name (1-64 English letters, digits or _)"
        )));
    }
    let target = write_target(&ctx.env, catalog)?;
    // a read-only target fails here, before anything is downloaded
    Catalogs::target_dir(&ctx.env, target)?;
    let api = BotApi::from_env()?;

    let set = api.get_sticker_set(short_name)?;
    if !is_set_name(&set.name) {
        return Err(Error::Runtime(
            "getStickerSet returned a set name that is not 1-64 English letters, digits or _"
                .into(),
        ));
    }
    let (name, title) = (set.name.clone(), set.title.clone());
    let fetched = set.into_fetched()?;
    if let Some(bad) = fetched
        .iter()
        .find(|f| !is_file_unique_id(&f.file_unique_id))
    {
        return Err(Error::Runtime(format!(
            "sticker {} of {name}: file_unique_id is not 1-64 English letters, digits, _ or -",
            bad.n
        )));
    }

    let sources = download_all(&api, &ctx.env.home.join("cache").join("src"), &fetched)?;
    let icons = render_all(&fetched, &sources)?;
    let (dir, pack, report, seeded_from) =
        write_pack(&ctx.env, target, &name, &title, &fetched, &icons)?;
    emit(ctx, &dir, &pack, &report, &icons, seeded_from.as_deref());
    Ok(())
}

fn write_target(env: &Env, catalog: Option<CatalogChoice>) -> Result<WriteTarget, Error> {
    match (&env.catalog_dir, catalog) {
        (Some(_), Some(_)) => Err(Error::Usage(
            "--catalog and --catalog-dir cannot be used together".into(),
        )),
        (Some(_), None) => Ok(WriteTarget::Explicit),
        (None, Some(CatalogChoice::Project)) => Ok(WriteTarget::Project),
        (None, Some(CatalogChoice::Local) | None) => Ok(WriteTarget::Local),
    }
}

/// Set names become folder and file names, so only `[A-Za-z0-9_]{1,64}` passes.
fn is_set_name(s: &str) -> bool {
    (1..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// A `file_unique_id` becomes a cache file name, so only `[A-Za-z0-9_-]{1,64}` passes.
fn is_file_unique_id(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn source_extension(format: Format) -> &'static str {
    match format {
        Format::Static => "webp",
        Format::Tgs => "tgs",
        Format::Webm => "webm",
    }
}

/// Phase A: the cached source of every emoji, downloaded only when missing.
fn download_all(
    api: &BotApi,
    cache: &Path,
    fetched: &[FetchedEmoji],
) -> Result<Vec<PathBuf>, Error> {
    fetched
        .iter()
        .map(|f| {
            let path = cache.join(format!(
                "{}.{}",
                f.file_unique_id,
                source_extension(f.format)
            ));
            if !path.is_file() {
                write_atomic(&path, &api.download(&f.file_id)?)?;
            }
            Ok(path)
        })
        .collect()
}

/// One emoji's preview icon, always `ICON`×`ICON`.
struct Rendered {
    image: Rgba,
    /// Coverage figures of the sampled frames, or the error that left the icon empty.
    stats: Result<FrameStats, String>,
}

impl Rendered {
    fn frame_suspect(&self) -> bool {
        match &self.stats {
            Ok(stats) => is_frame_suspect(stats),
            Err(_) => true,
        }
    }
}

/// Phase B: one icon per emoji id (the first occurrence wins, as in `merge`).
///
/// A source that fails to decode or render gets an empty icon and `frame_suspect`, and the build
/// goes on. Known limitation: rlottie aborts the whole process on some malformed but parseable
/// Lottie (a C++ assert, not an error we can catch); sources that are not gzip or not JSON fail
/// cleanly. Phases A and B write nothing to the catalog, so such an abort leaves no partial pack.
fn render_all(
    fetched: &[FetchedEmoji],
    sources: &[PathBuf],
) -> Result<HashMap<String, Rendered>, Error> {
    let mut icons = HashMap::new();
    for (f, path) in fetched.iter().zip(sources) {
        if icons.contains_key(&f.id) {
            continue;
        }
        let bytes = fs::read(path)
            .map_err(|e| Error::Runtime(format!("cannot read {}: {e}", path.display())))?;
        let rendered = match render_icon(f.format, &bytes, f.repaint) {
            Ok((image, stats)) => Rendered {
                image,
                stats: Ok(stats),
            },
            Err(err) => Rendered {
                image: Rgba::new(ICON, ICON),
                stats: Err(err.to_string()),
            },
        };
        icons.insert(f.id.clone(), rendered);
    }
    Ok(icons)
}

/// The best of up to `SAMPLES` evenly spaced frames (a static image has one), whitened for
/// `repaint` emoji.
fn render_icon(format: Format, bytes: &[u8], repaint: bool) -> Result<(Rgba, FrameStats), Error> {
    let mut media = Media::open(format, bytes)?;
    let mut frames = Vec::new();
    for index in sample_positions(media.frame_count(), SAMPLES) {
        let frame = media.frame(index, ICON)?;
        if (frame.w, frame.h) != (ICON, ICON) || frame.px.len() != (ICON * ICON * 4) as usize {
            return Err(Error::Runtime(format!(
                "frame {index} came out {}x{}, not {ICON}x{ICON}",
                frame.w, frame.h
            )));
        }
        frames.push(frame);
    }
    if frames.is_empty() {
        return Err(Error::Runtime("the source has no frames".into()));
    }
    let coverages: Vec<f32> = frames.iter().map(coverage).collect();
    let stats = pick_best(&coverages);
    let mut image = frames.swap_remove(stats.best_index);
    if repaint {
        whiten(&mut image);
    }
    Ok((image, stats))
}

/// Phase C: merge, swap in the new preview folder and write `pack.json`, under the pack's lock.
/// Returns the catalog directory, the written pack and the merge report.
fn write_pack(
    env: &Env,
    target: WriteTarget,
    name: &str,
    title: &str,
    fetched: &[FetchedEmoji],
    icons: &HashMap<String, Rendered>,
) -> Result<(PathBuf, Pack, MergeReport, Option<String>), Error> {
    let catalog_dir = Catalogs::writable_dir(env, target)?;
    let packs = catalog_dir.join("packs");
    let folder = pack_folder(&packs, name)?.unwrap_or_else(|| name.to_string());
    let pack_dir = packs.join(folder);
    let suspect: HashMap<String, bool> = icons
        .iter()
        .map(|(id, icon)| (id.clone(), icon.frame_suspect()))
        .collect();
    let now = utc_now();
    // a pack that exists only in a lower catalog must not be shadowed by an unlabeled copy
    let mut seed: Option<(String, Pack)> = None;
    if !pack_dir.join("pack.json").exists() {
        let (catalogs, _) = Catalogs::discover(env);
        seed = catalogs
            .find_pack(name)
            .map(|(catalog, pack)| (catalog.meta.id.clone(), pack));
    }
    let seeded_from = seed.as_ref().map(|(id, _)| id.clone());
    if let Some(id) = &seeded_from {
        eprintln!("pack {name}: starting from labels in catalog {id}");
    }
    let mut seed = seed.map(|(_, pack)| pack);
    let mut report = MergeReport::default();
    // dropped without `commit` (pack.json not written), it puts the previous previews back
    let mut swap: Option<PreviewSwap> = None;
    let pack = update_json(
        &pack_dir.join("pack.json"),
        MAX_PACK_JSON_BYTES,
        |existing: Option<Pack>| {
            let (pack, merged) = merge(
                existing.or_else(|| seed.take()),
                name,
                title,
                fetched,
                &suspect,
                &now,
            );
            report = merged;
            remove_staging_leftovers(&pack_dir);
            let staged = stage_previews(&pack_dir, &pack, icons)?;
            swap = Some(PreviewSwap::swap_in(&pack_dir, staged)?);
            Ok(pack)
        },
    )?;
    if let Some(swap) = swap {
        swap.commit();
    }
    Ok((catalog_dir, pack, report, seeded_from))
}

/// Staging folders left by a build that was killed mid-swap. Called under the pack's lock, so no
/// other build of this pack is using them.
fn remove_staging_leftovers(pack_dir: &Path) {
    let Ok(entries) = fs::read_dir(pack_dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(STAGING) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Renders every sheet of the present emoji into a fresh folder next to `preview/`; nothing is
/// left behind on failure.
fn stage_previews(
    pack_dir: &Path,
    pack: &Pack,
    icons: &HashMap<String, Rendered>,
) -> Result<PathBuf, Error> {
    let staged = pack_dir.join(format!("{STAGING}tmp-{}", std::process::id()));
    fs::create_dir_all(&staged)?;
    match write_sheets(&staged, pack, icons) {
        Ok(()) => Ok(staged),
        Err(err) => {
            let _ = fs::remove_dir_all(&staged);
            Err(err)
        }
    }
}

fn write_sheets(dir: &Path, pack: &Pack, icons: &HashMap<String, Rendered>) -> Result<(), Error> {
    let mut pages: BTreeMap<u32, Vec<(u32, &Rgba)>> = BTreeMap::new();
    for emoji in &pack.emoji {
        let Some(n) = emoji.n else {
            continue;
        };
        let icon = icons.get(&emoji.id).ok_or_else(|| {
            Error::Runtime(format!("no icon was rendered for emoji {}", emoji.id))
        })?;
        pages.entry(page_of(n)).or_default().push((n, &icon.image));
    }
    for (page, cells) in pages {
        let cells: Vec<(u32, Rgba)> = cells
            .into_iter()
            .map(|(n, image)| (n, image.clone()))
            .collect();
        write_png(
            &dir.join(sheet_file_name(&pack.short_name, page)),
            &compose(&cells),
        )?;
    }
    Ok(())
}

/// A staged preview folder moved into place. Until `commit`, dropping it puts the previous
/// folder back.
struct PreviewSwap {
    live: PathBuf,
    old: Option<PathBuf>,
    committed: bool,
}

impl PreviewSwap {
    /// Moves `preview/` aside and `staged` into its place. On failure `staged` is removed and
    /// the previous `preview/` restored when possible.
    fn swap_in(pack_dir: &Path, staged: PathBuf) -> Result<PreviewSwap, Error> {
        let live = pack_dir.join(PREVIEW);
        let aside = pack_dir.join(format!("{STAGING}old-{}", std::process::id()));
        let old = match fs::rename(&live, &aside) {
            Ok(()) => Some(aside),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => {
                let _ = fs::remove_dir_all(&staged);
                return Err(err.into());
            }
        };
        if let Err(err) = fs::rename(&staged, &live) {
            let _ = fs::remove_dir_all(&staged);
            if let Some(old) = &old {
                let _ = fs::rename(old, &live);
            }
            return Err(err.into());
        }
        Ok(PreviewSwap {
            live,
            old,
            committed: false,
        })
    }

    /// Keeps the new folder and removes the previous one with its stale pages.
    fn commit(mut self) {
        self.committed = true;
        if let Some(old) = &self.old {
            let _ = fs::remove_dir_all(old);
        }
    }
}

impl Drop for PreviewSwap {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let _ = fs::remove_dir_all(&self.live);
        if let Some(old) = &self.old {
            let _ = fs::rename(old, &self.live);
        }
    }
}

fn emit(
    ctx: &Ctx,
    dir: &Path,
    pack: &Pack,
    report: &MergeReport,
    icons: &HashMap<String, Rendered>,
    seeded_from: Option<&str>,
) {
    let mut errors = Vec::new();
    let mut frames = Vec::new();
    let mut lines = vec![
        format!("pack {} built in {}", pack.short_name, dir.display()),
        format!(
            "new {}, changed {}, gone {}, returned {}, total {}",
            report.added.len(),
            report.image_changed.len(),
            report.gone.len(),
            report.returned.len(),
            report.total
        ),
    ];
    for emoji in pack.emoji.iter().filter(|e| e.n.is_some()) {
        match icons.get(&emoji.id).map(|icon| &icon.stats) {
            Some(Ok(s)) => frames.push(json!({
                "id": emoji.id,
                "best_index": s.best_index,
                "best": s.best,
                "median": s.median,
                "first": s.first,
            })),
            Some(Err(error)) => {
                lines.push(format!("decode error {}: {error}", emoji.id));
                errors.push(json!({ "id": emoji.id, "error": error }));
            }
            None => {}
        }
    }
    ctx.emit(
        &lines.join("\n"),
        json!({
            "pack": pack.short_name,
            "dir": dir.display().to_string(),
            "seeded_from": seeded_from,
            "report": {
                "added": report.added,
                "image_changed": report.image_changed,
                "gone": report.gone,
                "returned": report.returned,
                "total": report.total,
            },
            "errors": errors,
            "frames": frames,
        }),
    );
}

/// The current UTC time as ISO 8601.
fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    iso8601(secs)
}

/// Seconds since the Unix epoch as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// Days to a civil date by Howard Hinnant's `civil_from_days`: shift the epoch to 0000-03-01 so
/// the leap day ends the year, split into 400-year eras of 146097 days, then find the year of
/// the era and the day of that March-based year.
fn iso8601(secs: u64) -> String {
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let z = days + 719_468;
    let (era, doe) = (z / 146_097, z % 146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    /// A pack folder with `preview/old_1.png` and a staged folder holding `new_1.png`.
    fn staged_pack() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(PREVIEW)).unwrap();
        fs::write(dir.path().join("preview/old_1.png"), b"old").unwrap();
        let staged = dir.path().join(".preview-tmp-test");
        fs::create_dir(&staged).unwrap();
        fs::write(staged.join("new_1.png"), b"new").unwrap();
        (dir, staged)
    }

    #[test]
    fn committed_swap_drops_the_previous_folder() {
        let (dir, staged) = staged_pack();
        PreviewSwap::swap_in(dir.path(), staged).unwrap().commit();
        assert_eq!(names(dir.path()), [PREVIEW]);
        assert_eq!(names(&dir.path().join(PREVIEW)), ["new_1.png"]);
    }

    #[test]
    fn uncommitted_swap_restores_the_previous_folder() {
        let (dir, staged) = staged_pack();
        let swap = PreviewSwap::swap_in(dir.path(), staged).unwrap();
        assert_eq!(names(&dir.path().join(PREVIEW)), ["new_1.png"]);
        drop(swap);
        assert_eq!(names(dir.path()), [PREVIEW]);
        assert_eq!(names(&dir.path().join(PREVIEW)), ["old_1.png"]);
    }

    #[test]
    fn failed_swap_keeps_the_live_folder() {
        let (dir, _) = staged_pack();
        let missing = dir.path().join(".preview-tmp-missing");
        assert!(PreviewSwap::swap_in(dir.path(), missing).is_err());
        assert_eq!(names(dir.path()), [".preview-tmp-test", PREVIEW]);
        assert_eq!(names(&dir.path().join(PREVIEW)), ["old_1.png"]);
    }

    #[test]
    fn iso8601_formats_utc() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso8601(1_709_210_096), "2024-02-29T12:34:56Z");
        assert_eq!(iso8601(1_767_225_599), "2025-12-31T23:59:59Z");
        assert_eq!(iso8601(1_793_491_200), "2026-11-01T00:00:00Z");
    }
}
