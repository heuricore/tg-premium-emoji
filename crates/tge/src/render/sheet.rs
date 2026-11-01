//! Numbered preview sheets: icons in a 5-column grid with their number under each.

use std::path::Path;

use super::{Rgba, font};
use crate::error::Error;

pub const CELL: u32 = 144;
pub const ICON: u32 = 128;
pub const LABEL_H: u32 = 24;
pub const COLS: u32 = 5;
pub const PER_SHEET: u32 = 20;
pub const BG: [u8; 4] = [0x1E, 0x21, 0x27, 0xFF];
pub const FG: [u8; 4] = [0xE6, 0xE6, 0xE6, 0xFF];

/// Number of sheets needed for `k` emoji.
pub fn sheet_count(k: u32) -> u32 {
    k.div_ceil(PER_SHEET)
}

pub fn sheet_file_name(short_name: &str, page: u32) -> String {
    format!("{short_name}_{page}.png")
}

/// Sheet page (from 1) holding the 1-based emoji number `n`.
pub fn page_of(n: u32) -> u32 {
    n.saturating_sub(1) / PER_SHEET + 1
}

/// Lays `cells` out by position in a `COLS`-wide grid, each icon centred above its number.
/// Every pixel of the result is opaque.
pub fn compose(cells: &[(u32, Rgba)]) -> Rgba {
    let rows = (cells.len() as u32).div_ceil(COLS);
    let (w, h) = (COLS * CELL, rows * (CELL + LABEL_H));
    let mut img = Rgba {
        w,
        h,
        px: BG.repeat(w as usize * h as usize),
    };
    for (k, (n, icon)) in cells.iter().enumerate() {
        let k = k as u32;
        let (cell_x, cell_y) = ((k % COLS) * CELL, (k / COLS) * (CELL + LABEL_H));
        blit_centred(&mut img, icon, cell_x, cell_y);
        font::draw_number(
            &mut img,
            cell_x + CELL / 2,
            cell_y + CELL + (LABEL_H - 14) / 2,
            *n,
        );
    }
    img
}

/// Alpha-composites `icon` centred in the `CELL`×`CELL` area at (`cell_x`, `cell_y`), clipping
/// to that area.
fn blit_centred(img: &mut Rgba, icon: &Rgba, cell_x: u32, cell_y: u32) {
    if icon.px.len() != icon.w as usize * icon.h as usize * 4 {
        return;
    }
    let off_x = (i64::from(CELL) - i64::from(icon.w)) / 2;
    let off_y = (i64::from(CELL) - i64::from(icon.h)) / 2;
    for sy in 0..icon.h {
        let dy = off_y + i64::from(sy);
        if !(0..i64::from(CELL)).contains(&dy) {
            continue;
        }
        for sx in 0..icon.w {
            let dx = off_x + i64::from(sx);
            if !(0..i64::from(CELL)).contains(&dx) {
                continue;
            }
            let s = (sy as usize * icon.w as usize + sx as usize) * 4;
            let d =
                ((cell_y as usize + dy as usize) * img.w as usize + cell_x as usize + dx as usize)
                    * 4;
            let a = u32::from(icon.px[s + 3]);
            for c in 0..3 {
                let (src, dst) = (u32::from(icon.px[s + c]), u32::from(img.px[d + c]));
                img.px[d + c] = ((src * a + dst * (255 - a) + 127) / 255) as u8;
            }
        }
    }
}

/// Encodes `img` as RGBA8 PNG and writes it atomically.
pub fn write_png(path: &Path, img: &Rgba) -> Result<(), Error> {
    if img.w == 0 || img.h == 0 {
        return Err(Error::Runtime(format!(
            "cannot write a {}x{} image as PNG",
            img.w, img.h
        )));
    }
    if img.px.len() != img.w as usize * img.h as usize * 4 {
        return Err(Error::Runtime(format!(
            "image buffer holds {} bytes, expected {} for {}x{}",
            img.px.len(),
            img.w as usize * img.h as usize * 4,
            img.w,
            img.h
        )));
    }
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, img.w, img.h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let encode = |e: png::Encoder<&mut Vec<u8>>| -> Result<(), png::EncodingError> {
        let mut writer = e.write_header()?;
        writer.write_image_data(&img.px)?;
        writer.finish()
    };
    encode(encoder).map_err(|e| Error::Runtime(format!("PNG encoding failed: {e}")))?;
    crate::store::atomic::write_atomic(path, &bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon(color: [u8; 4]) -> Rgba {
        Rgba {
            w: ICON,
            h: ICON,
            px: color.repeat((ICON * ICON) as usize),
        }
    }

    fn cells(k: u32) -> Vec<(u32, Rgba)> {
        (1..=k).map(|n| (n, icon([200, 10, 10, 255]))).collect()
    }

    fn pixel(img: &Rgba, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * img.w + x) * 4) as usize;
        img.px[at..at + 4].try_into().unwrap()
    }

    fn has_fg(img: &Rgba, x0: u32, y0: u32, w: u32, h: u32) -> bool {
        (y0..y0 + h).any(|y| (x0..x0 + w).any(|x| pixel(img, x, y) == FG))
    }

    #[test]
    fn full_sheet_size() {
        let img = compose(&cells(20));
        assert_eq!((img.w, img.h), (720, 672));
    }

    #[test]
    fn partial_sheet_size() {
        let img = compose(&cells(7));
        assert_eq!((img.w, img.h), (720, 336));
    }

    #[test]
    fn empty_input_is_720_wide_and_flat() {
        let img = compose(&[]);
        assert_eq!((img.w, img.h, img.px.len()), (720, 0, 0));
    }

    #[test]
    fn more_cells_than_a_sheet_add_rows() {
        let img = compose(&cells(25));
        assert_eq!((img.w, img.h), (720, 5 * 168));
    }

    #[test]
    fn number_is_drawn() {
        let img = compose(&cells(1));
        assert!(has_fg(&img, 0, CELL, CELL, LABEL_H));
        assert!(!has_fg(&img, CELL, CELL, CELL, LABEL_H));
    }

    #[test]
    fn number_is_centred_in_label_strip() {
        let img = compose(&cells(1));
        let top = CELL + (LABEL_H - 14) / 2;
        assert!(has_fg(&img, 0, top, CELL, 14));
        assert!(!has_fg(&img, 0, CELL, CELL, top - CELL));
        assert!(!has_fg(&img, 0, top + 14, CELL, CELL + LABEL_H - top - 14));
    }

    #[test]
    fn icon_is_centred_and_empty_cells_are_background() {
        let img = compose(&cells(1));
        assert_eq!(pixel(&img, 8, 8), [200, 10, 10, 255]);
        assert_eq!(pixel(&img, 7, 8), BG);
        assert_eq!(pixel(&img, 8 + ICON, 8), BG);
        assert_eq!(pixel(&img, CELL + 8, 8), BG);
    }

    #[test]
    fn alpha_is_blended_over_background() {
        let half = icon([255, 255, 255, 128]);
        let img = compose(&[(1, half)]);
        let p = pixel(&img, 20, 20);
        assert_eq!(p[3], 255);
        assert_eq!(p[0], ((255u32 * 128 + 0x1E * 127 + 127) / 255) as u8);
        assert!(p[0] > BG[0] && p[0] < 255);
        let clear = compose(&[(1, icon([255, 255, 255, 0]))]);
        assert_eq!(pixel(&clear, 20, 20), BG);
    }

    #[test]
    fn odd_sized_icons_are_clipped_not_panicking() {
        let big = Rgba {
            w: 300,
            h: 200,
            px: [9, 9, 9, 255].repeat(300 * 200),
        };
        let small = Rgba {
            w: 10,
            h: 10,
            px: [9, 9, 9, 255].repeat(100),
        };
        let img = compose(&[(1, big), (2, small), (3, Rgba::new(0, 0))]);
        assert_eq!((img.w, img.h), (720, 168));
        assert_eq!(pixel(&img, CELL, 0), BG);
        assert_eq!(pixel(&img, CELL + 72, 72), [9, 9, 9, 255]);
        assert_eq!(pixel(&img, CELL * 2 + 72, 72), BG);
    }

    #[test]
    fn multi_digit_numbers_are_drawn() {
        let img = compose(&[(1234, icon([0, 0, 0, 255]))]);
        assert!(has_fg(&img, 0, CELL, CELL, LABEL_H));
        assert!(!has_fg(&img, CELL, CELL, CELL, LABEL_H));
    }

    #[test]
    fn paging() {
        assert_eq!(page_of(1), 1);
        assert_eq!(page_of(20), 1);
        assert_eq!(page_of(21), 2);
        assert_eq!(sheet_count(0), 0);
        assert_eq!(sheet_count(20), 1);
        assert_eq!(sheet_count(41), 3);
        assert_eq!(sheet_file_name("tgiosicons", 2), "tgiosicons_2.png");
    }

    #[test]
    fn png_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s_1.png");
        let img = compose(&cells(3));
        write_png(&path, &img).unwrap();
        let decoder =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (720, 168));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(&buf[..info.buffer_size()], &img.px[..]);
        let at = ((8 * 720 + 8) * 4) as usize;
        assert_eq!(&buf[at..at + 4], &[200, 10, 10, 255]);
    }

    #[test]
    fn write_png_rejects_bad_images() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.png");
        assert!(write_png(&path, &Rgba::new(0, 5)).is_err());
        assert!(write_png(&path, &Rgba::new(5, 0)).is_err());
        let bad = Rgba {
            w: 2,
            h: 2,
            px: vec![0; 3],
        };
        assert!(write_png(&path, &bad).is_err());
        assert!(!path.exists());
    }
}
