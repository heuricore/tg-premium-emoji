//! Built-in 5×7 digit bitmaps for the sheet labels.

use super::Rgba;
use super::sheet::FG;

const GLYPH_W: u32 = 5;
const SCALE: u32 = 2;
const GAP: u32 = 2;

/// Rows top to bottom, bit 4 is the leftmost column.
const DIGITS: [[u8; 7]; 10] = [
    [
        0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
    ],
    [
        0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
    ],
    [
        0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
    ],
    [
        0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
    ],
    [
        0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
    ],
    [
        0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
    ],
    [
        0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
    ],
    [
        0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
    ],
    [
        0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
    ],
    [
        0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
    ],
];

/// Draws decimal `n` in the label colour, horizontally centred on `cx` with its top row at
/// `top`. Pixels outside the image are skipped.
pub fn draw_number(img: &mut Rgba, cx: u32, top: u32, n: u32) {
    let text = n.to_string();
    let digit_w = GLYPH_W * SCALE;
    let count = text.len() as u32;
    let total = count * digit_w + (count - 1) * GAP;
    let left = i64::from(cx) - i64::from(total / 2);
    for (i, ch) in text.bytes().enumerate() {
        let glyph = &DIGITS[(ch - b'0') as usize];
        let x0 = left + i64::from(i as u32 * (digit_w + GAP));
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..GLYPH_W {
                if bits >> (GLYPH_W - 1 - col) & 1 == 0 {
                    continue;
                }
                for dy in 0..SCALE {
                    for dx in 0..SCALE {
                        let x = x0 + i64::from(col * SCALE + dx);
                        let y = i64::from(top) + i64::from(row as u32 * SCALE + dy);
                        put(img, x, y);
                    }
                }
            }
        }
    }
}

fn put(img: &mut Rgba, x: i64, y: i64) {
    if x < 0 || y < 0 || x >= i64::from(img.w) || y >= i64::from(img.h) {
        return;
    }
    let at = (y as usize * img.w as usize + x as usize) * 4;
    if let Some(px) = img.px.get_mut(at..at + 4) {
        px.copy_from_slice(&FG);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_are_distinct() {
        for (a, ga) in DIGITS.iter().enumerate() {
            for (b, gb) in DIGITS.iter().enumerate().skip(a + 1) {
                assert_ne!(ga, gb, "{a} and {b}");
            }
        }
    }
}
