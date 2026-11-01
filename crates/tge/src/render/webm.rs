//! webm emoji: VP9 video whose alpha channel travels as a second VP9 stream in BlockAdditional
//! (BlockAddID 1), as libvpx-vp9 writes it. A minimal EBML reader takes the packets of the first
//! video track; two vp9dec decoders turn the colour and alpha streams into YUVA frames.

use std::fmt::Display;
use std::panic::{self, AssertUnwindSafe};

use vp9dec::PlaneData;

use super::{Rgba, check_source_size};
use crate::error::Error;

/// Cap on the decoded frames of one file, `FRAME_OVERHEAD` per frame included.
const MAX_DECODED_BYTES: usize = 256 * 1024 * 1024;

/// Charged per frame on top of its planes: the frame record and its four allocations.
const FRAME_OVERHEAD: usize = 256;

/// One decoded 8-bit 4:2:0 frame, planes row by row from the top: `y` and `a` hold `w`×`h`
/// samples, `u` and `v` hold `ceil(w/2)`×`ceil(h/2)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YuvaFrame {
    pub w: u32,
    pub h: u32,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
    pub a: Vec<u8>,
}

/// Every shown frame of a webm, in display order. A block without an alpha frame is opaque.
pub fn decode_yuva(bytes: &[u8]) -> Result<Vec<YuvaFrame>, Error> {
    decode_packets(&demux(bytes)?, MAX_DECODED_BYTES)
}

/// BT.601 limited range, the swscale default for VP9 without colour metadata, to straight RGBA.
/// The planes must have the sizes `decode_yuva` gives them.
pub fn to_rgba(f: &YuvaFrame) -> Rgba {
    let (w, cw) = (f.w as usize, f.w.div_ceil(2) as usize);
    let clip = |x: i32| x.clamp(0, 255) as u8;
    let mut img = Rgba::new(f.w, f.h);
    for (i, px) in img.px.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (row, col) = (i / w, i % w);
        let chroma = row / 2 * cw + col / 2;
        let c = i32::from(f.y[i]) - 16;
        let d = i32::from(f.u[chroma]) - 128;
        let e = i32::from(f.v[chroma]) - 128;
        *px = [
            clip((298 * c + 409 * e + 128) >> 8),
            clip((298 * c - 100 * d - 208 * e + 128) >> 8),
            clip((298 * c + 516 * d + 128) >> 8),
            f.a[i],
        ];
    }
    img
}

// ---- EBML ----

const EBML: u32 = 0x1A45_DFA3;
const SEGMENT: u32 = 0x1853_8067;
const TRACKS: u32 = 0x1654_AE6B;
const TRACK_ENTRY: u32 = 0xAE;
const TRACK_NUMBER: u32 = 0xD7;
const CODEC_ID: u32 = 0x86;
const VIDEO: u32 = 0xE0;
const CLUSTER: u32 = 0x1F43_B675;
const SIMPLE_BLOCK: u32 = 0xA3;
const BLOCK_GROUP: u32 = 0xA0;
const BLOCK: u32 = 0xA1;
const BLOCK_ADDITIONS: u32 = 0x75A1;
const BLOCK_MORE: u32 = 0xA6;
const BLOCK_ADD_ID: u32 = 0xEE;
const BLOCK_ADDITIONAL: u32 = 0xA5;

fn invalid(what: impl Display) -> Error {
    Error::Runtime(format!("invalid webm: {what}"))
}

/// An element ID and the byte range of its body.
struct Element {
    id: u32,
    start: usize,
    end: usize,
}

/// An EBML variable-length integer at `pos` with its length marker kept, and its length. It must
/// end by `end`.
fn vint(buf: &[u8], pos: usize, end: usize) -> Result<(u64, usize), Error> {
    let malformed = || invalid(format_args!("malformed element header at byte {pos}"));
    let first = *buf[..end].get(pos).ok_or_else(malformed)?;
    let len = first.leading_zeros() as usize + 1;
    let bytes = buf[..end]
        .get(pos..pos + len)
        .filter(|_| len <= 8)
        .ok_or_else(malformed)?;
    Ok((
        bytes.iter().fold(0, |acc, &b| (acc << 8) | u64::from(b)),
        len,
    ))
}

/// The value of a `vint` with the length marker dropped.
fn vint_value(raw: u64, len: usize) -> u64 {
    raw & ((1 << (7 * len)) - 1)
}

/// The element whose header starts at `pos`. Its body must end by `parent_end`; only the Segment
/// may have an unknown size, and then it extends to `parent_end`.
fn element(buf: &[u8], pos: usize, parent_end: usize) -> Result<Element, Error> {
    let (id, id_len) = vint(buf, pos, parent_end)?;
    if id_len > 4 {
        return Err(invalid(format_args!(
            "element ID at byte {pos} is too long"
        )));
    }
    let id = id as u32;
    let (raw_size, size_len) = vint(buf, pos + id_len, parent_end)?;
    let start = pos + id_len + size_len;
    let size = vint_value(raw_size, size_len);
    let end = if size == vint_value(u64::MAX, size_len) {
        if id != SEGMENT {
            return Err(invalid(format_args!(
                "element {id:#X} at byte {pos} has an unknown size, which only the Segment may have"
            )));
        }
        parent_end
    } else {
        usize::try_from(size)
            .ok()
            .and_then(|size| start.checked_add(size))
            .filter(|&end| end <= parent_end)
            .ok_or_else(|| {
                invalid(format_args!(
                    "element {id:#X} at byte {pos} overruns its parent"
                ))
            })?
    };
    Ok(Element { id, start, end })
}

/// The elements in `buf[pos..end]`; the first malformed one ends the iteration with an error.
struct Children<'a> {
    buf: &'a [u8],
    pos: usize,
    end: usize,
}

impl Iterator for Children<'_> {
    type Item = Result<Element, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.end {
            return None;
        }
        let item = element(self.buf, self.pos, self.end);
        self.pos = item.as_ref().map_or(self.end, |el| el.end);
        Some(item)
    }
}

/// One block of the video track: its VP9 data and the alpha stream's VP9 data, if any.
struct Packet<'a> {
    data: &'a [u8],
    alpha: Option<&'a [u8]>,
}

struct Track<'a> {
    number: u64,
    codec: &'a [u8],
}

/// Walks the fixed element hierarchy EBML header, Segment, Tracks or Cluster, block. The depth is
/// bounded by construction, so nesting in a hostile file cannot exhaust the stack.
struct Demuxer<'a> {
    buf: &'a [u8],
    track: Option<Track<'a>>,
    packets: Vec<Packet<'a>>,
}

/// The packets of the first video track, in file order. Blocks before the Tracks element are
/// skipped, as WebM puts Tracks first.
fn demux(buf: &[u8]) -> Result<Vec<Packet<'_>>, Error> {
    let mut top = Children {
        buf,
        pos: 0,
        end: buf.len(),
    };
    if top.next().transpose()?.map(|el| el.id) != Some(EBML) {
        return Err(invalid("no EBML header"));
    }
    let mut demuxer = Demuxer {
        buf,
        track: None,
        packets: Vec::new(),
    };
    for el in top {
        let el = el?;
        if el.id == SEGMENT {
            demuxer.segment(&el)?;
        }
    }
    let track = demuxer.track.ok_or_else(|| invalid("no video track"))?;
    if track.codec != b"V_VP9" {
        return Err(Error::Runtime(format!(
            "webm video codec {} is not supported, only V_VP9",
            String::from_utf8_lossy(track.codec)
        )));
    }
    Ok(demuxer.packets)
}

impl<'a> Demuxer<'a> {
    fn children(&self, parent: &Element) -> Children<'a> {
        Children {
            buf: self.buf,
            pos: parent.start,
            end: parent.end,
        }
    }

    fn body(&self, el: &Element) -> &'a [u8] {
        &self.buf[el.start..el.end]
    }

    fn uint(&self, el: &Element) -> Result<u64, Error> {
        let body = self.body(el);
        if body.len() > 8 {
            return Err(invalid(format_args!(
                "integer element {:#X} is too long",
                el.id
            )));
        }
        Ok(body.iter().fold(0, |acc, &b| (acc << 8) | u64::from(b)))
    }

    fn segment(&mut self, segment: &Element) -> Result<(), Error> {
        for el in self.children(segment) {
            let el = el?;
            match el.id {
                TRACKS => self.tracks(&el)?,
                CLUSTER => self.cluster(&el)?,
                _ => {}
            }
        }
        Ok(())
    }

    /// Keeps the first track entry with video settings.
    fn tracks(&mut self, tracks: &Element) -> Result<(), Error> {
        for entry in self.children(tracks) {
            let entry = entry?;
            if entry.id != TRACK_ENTRY || self.track.is_some() {
                continue;
            }
            let (mut number, mut codec, mut video) = (0, &[][..], false);
            for el in self.children(&entry) {
                let el = el?;
                match el.id {
                    TRACK_NUMBER => number = self.uint(&el)?,
                    CODEC_ID => codec = self.body(&el),
                    VIDEO => video = true,
                    _ => {}
                }
            }
            if video {
                self.track = Some(Track { number, codec });
            }
        }
        Ok(())
    }

    fn cluster(&mut self, cluster: &Element) -> Result<(), Error> {
        for el in self.children(cluster) {
            let el = el?;
            match el.id {
                SIMPLE_BLOCK => {
                    if let Some(data) = self.block(&el)? {
                        self.packets.push(Packet { data, alpha: None });
                    }
                }
                BLOCK_GROUP => self.block_group(&el)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn block_group(&mut self, group: &Element) -> Result<(), Error> {
        let (mut data, mut alpha) = (None, None);
        for el in self.children(group) {
            let el = el?;
            match el.id {
                BLOCK => data = self.block(&el)?,
                BLOCK_ADDITIONS => alpha = self.alpha(&el)?,
                _ => {}
            }
        }
        if let Some(data) = data {
            self.packets.push(Packet { data, alpha });
        }
        Ok(())
    }

    /// The payload of the BlockAdditional with BlockAddID 1 (its default), the alpha frame.
    fn alpha(&self, additions: &Element) -> Result<Option<&'a [u8]>, Error> {
        for more in self.children(additions) {
            let more = more?;
            if more.id != BLOCK_MORE {
                continue;
            }
            let (mut add_id, mut payload) = (1, None);
            for el in self.children(&more) {
                let el = el?;
                match el.id {
                    BLOCK_ADD_ID => add_id = self.uint(&el)?,
                    BLOCK_ADDITIONAL => payload = Some(self.body(&el)),
                    _ => {}
                }
            }
            if add_id == 1 && payload.is_some() {
                return Ok(payload);
            }
        }
        Ok(None)
    }

    /// The frame data of a Block or SimpleBlock on the video track; `None` on other tracks.
    fn block(&self, block: &Element) -> Result<Option<&'a [u8]>, Error> {
        let (raw, len) = vint(self.buf, block.start, block.end)?;
        // track number, 2-byte timecode, flags
        let data_start = block.start + len + 3;
        if data_start > block.end {
            return Err(invalid("truncated block header"));
        }
        let number = vint_value(raw, len);
        if self.track.as_ref().is_none_or(|t| t.number != number) {
            return Ok(None);
        }
        if self.buf[data_start - 1] & 0x06 != 0 {
            return Err(invalid("a block uses lacing, which is not supported"));
        }
        Ok(Some(&self.buf[data_start..block.end]))
    }
}

// ---- VP9 ----

fn corrupt(what: impl Display) -> Error {
    Error::Runtime(format!("corrupt VP9 frame: {what}"))
}

/// Decodes the colour and alpha streams side by side and fails once the frames would take more
/// than `budget` bytes.
fn decode_packets(packets: &[Packet], budget: usize) -> Result<Vec<YuvaFrame>, Error> {
    let mut colour = vp9dec::Decoder::new();
    let mut alpha = vp9dec::Decoder::new();
    let mut frames = Vec::new();
    let mut used = 0usize;
    for packet in packets {
        let shown = decode_chunk(&mut colour, packet.data)?;
        let shown_alpha = match packet.alpha {
            Some(data) => Some(decode_chunk(&mut alpha, data)?),
            None => None,
        };
        let frame = match (shown, shown_alpha) {
            (None, None | Some(None)) => continue,
            (Some(c), None) => to_yuva(c, None)?,
            (Some(c), Some(Some(a))) => to_yuva(c, Some(a))?,
            (Some(_), Some(None)) | (None, Some(Some(_))) => {
                return Err(Error::Runtime(
                    "webm alpha stream is out of step with the colour stream".into(),
                ));
            }
        };
        used = used.saturating_add(
            frame.y.len() + frame.u.len() + frame.v.len() + frame.a.len() + FRAME_OVERHEAD,
        );
        if used > budget {
            return Err(Error::Runtime(format!(
                "decoded webm frames exceed the {budget}-byte limit"
            )));
        }
        frames.push(frame);
    }
    if frames.is_empty() {
        return Err(Error::Runtime("webm has no frames".into()));
    }
    Ok(frames)
}

/// Decodes one block's VP9 data and returns the frame it shows, if any.
fn decode_chunk(
    decoder: &mut vp9dec::Decoder,
    chunk: &[u8],
) -> Result<Option<vp9dec::Frame>, Error> {
    // The split vp9dec applies itself (the crate is pinned to an exact version), so every frame
    // it is about to allocate for is checked first.
    for frame in vp9dec::superframe::split_superframe(chunk) {
        check_header(frame)?;
    }
    // vp9dec is young and may panic on a corrupt stream; a decoder that failed is not used again.
    let decoded = panic::catch_unwind(AssertUnwindSafe(|| decoder.decode_frame(chunk)))
        .map_err(|_| corrupt("the decoder panicked"))?
        .map_err(|e| corrupt(format_args!("{e:?}")))?;
    Ok(decoded.into_iter().find_map(|d| d.frame))
}

/// MSB-first bit reader that, like vp9dec's, reads zeros past the end.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn read(&mut self, count: u32) -> u32 {
        (0..count).fold(0, |acc, _| {
            let byte = self.data.get(self.pos / 8).copied().unwrap_or(0);
            let bit = (byte >> (7 - self.pos % 8)) & 1;
            self.pos += 1;
            (acc << 1) | u32::from(bit)
        })
    }

    fn flag(&mut self) -> bool {
        self.read(1) == 1
    }
}

const SYNC_CODE: u32 = 0x49_83_42;
const CS_RGB: u32 = 7;

/// Reads a VP9 uncompressed header (spec §6.2) up to the frame size and refuses, before vp9dec
/// allocates for it, a profile other than 0 or a frame beyond `MAX_SOURCE_SIDE`. Frames that show
/// an existing frame or reuse a reference's size need no check: every earlier frame passed it.
fn check_header(frame: &[u8]) -> Result<(), Error> {
    let mut r = Bits {
        data: frame,
        pos: 0,
    };
    if r.read(2) != 2 {
        return Err(corrupt("bad frame marker"));
    }
    let low = r.read(1);
    let profile = (r.read(1) << 1) | low;
    if profile != 0 {
        return Err(Error::Runtime(format!(
            "VP9 profile {profile} is not supported, only profile 0 (8-bit 4:2:0)"
        )));
    }
    if r.flag() {
        return Ok(()); // show_existing_frame
    }
    let key_frame = !r.flag();
    let show_frame = r.flag();
    let error_resilient = r.flag();
    let intra_only = !key_frame && !show_frame && r.flag();
    if !key_frame && !error_resilient {
        r.read(2); // reset_frame_context
    }
    if key_frame || intra_only {
        if r.read(24) != SYNC_CODE {
            return Err(corrupt("bad sync code"));
        }
        if key_frame {
            // color_config of profile 0: color_space, then color_range (sRGB needs profile 1 or 3)
            if r.read(3) == CS_RGB {
                return Err(corrupt("RGB colour space in profile 0"));
            }
            r.read(1);
        } else {
            r.read(8); // refresh_frame_flags; profile 0 intra-only frames carry no color_config
        }
    } else {
        r.read(8 + 3 * 4); // refresh_frame_flags, then ref_frame_idx and sign bias of 3 references
        if (0..3).any(|_| r.flag()) {
            return Ok(()); // found_ref
        }
    }
    let w = r.read(16) + 1;
    let h = r.read(16) + 1;
    check_source_size("VP9", w, h)
}

/// Pairs a shown colour frame with its alpha frame, whose luma plane is the alpha plane.
fn to_yuva(colour: vp9dec::Frame, alpha: Option<vp9dec::Frame>) -> Result<YuvaFrame, Error> {
    let (w, h) = (colour.width, colour.height);
    check_source_size("webm", w, h)?;
    let unsupported = || Error::Runtime("only 8-bit 4:2:0 VP9 is supported".into());
    if (colour.subsampling_x, colour.subsampling_y) != (1, 1) {
        return Err(unsupported());
    }
    let (PlaneData::U8(y), PlaneData::U8(u), PlaneData::U8(v)) = (colour.y, colour.u, colour.v)
    else {
        return Err(unsupported());
    };
    let luma = w as usize * h as usize;
    let chroma = w.div_ceil(2) as usize * h.div_ceil(2) as usize;
    if (y.len(), u.len(), v.len()) != (luma, chroma, chroma) {
        return Err(corrupt("planes do not match the frame size"));
    }
    let a = match alpha {
        None => vec![255; luma],
        Some(alpha) if (alpha.width, alpha.height) != (w, h) => {
            return Err(Error::Runtime(format!(
                "webm alpha frame is {}x{}, its colour frame {w}x{h}",
                alpha.width, alpha.height
            )));
        }
        Some(alpha) => match alpha.y {
            PlaneData::U8(a) if a.len() == luma => a,
            PlaneData::U8(_) => return Err(corrupt("alpha plane does not match the frame size")),
            PlaneData::U16(_) => return Err(unsupported()),
        },
    };
    Ok(YuvaFrame { w, h, y, u, v, a })
}

#[cfg(test)]
mod tests {
    use vp9dec::{Frame, PlaneData};

    use super::*;

    const ALPHA_WEBM: &[u8] = include_bytes!("../../tests/fixtures/alpha.webm");
    const SYNC: (u32, u32) = (0x49_83_42, 24);

    /// Packs `(value, bit count)` fields MSB first, as a VP9 uncompressed header.
    fn bits(fields: &[(u32, u32)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut n = 0;
        for &(value, count) in fields {
            for i in (0..count).rev() {
                if n % 8 == 0 {
                    out.push(0);
                }
                let bit = u8::from((value >> i) & 1 == 1);
                *out.last_mut().unwrap() |= bit << (7 - n % 8);
                n += 1;
            }
        }
        out
    }

    /// Profile 0 shown key frame, BT.601 colour space.
    fn key_frame(w: u32, h: u32) -> Vec<u8> {
        let head = [(2, 2), (0, 1), (0, 1), (0, 1), (0, 1), (1, 1), (0, 1)];
        bits(&[&head[..], &[SYNC, (1, 3), (0, 1), (w - 1, 16), (h - 1, 16)]].concat())
    }

    fn intra_only_frame(w: u32, h: u32) -> Vec<u8> {
        let head = [(2, 2), (0, 1), (0, 1), (0, 1), (1, 1), (0, 1), (0, 1)];
        let rest = [(1, 1), (0, 2), SYNC, (0xFF, 8), (w - 1, 16), (h - 1, 16)];
        bits(&[&head[..], &rest].concat())
    }

    /// Shown inter frame; `found_ref` bits precede the explicit size.
    fn inter_frame(error_resilient: bool, found_ref: &[u32], w: u32, h: u32) -> Vec<u8> {
        let mut fields = vec![(2, 2), (0, 1), (0, 1), (0, 1), (1, 1), (1, 1)];
        fields.push((u32::from(error_resilient), 1));
        if !error_resilient {
            fields.push((0, 2));
        }
        fields.extend([(1, 8), (0, 12)]);
        fields.extend(found_ref.iter().map(|&bit| (bit, 1)));
        fields.extend([(w - 1, 16), (h - 1, 16)]);
        bits(&fields)
    }

    fn frame(w: u32, h: u32) -> Frame {
        let chroma = (w.div_ceil(2) * h.div_ceil(2)) as usize;
        Frame {
            width: w,
            height: h,
            bit_depth: 8,
            subsampling_x: 1,
            subsampling_y: 1,
            y: PlaneData::U8(vec![50; (w * h) as usize]),
            u: PlaneData::U8(vec![60; chroma]),
            v: PlaneData::U8(vec![70; chroma]),
        }
    }

    #[test]
    fn header_sizes_within_limit_pass() {
        let ok = [
            key_frame(100, 100),
            key_frame(1024, 1024),
            intra_only_frame(64, 64),
            inter_frame(false, &[0, 0, 0], 50, 50),
            inter_frame(true, &[0, 0, 0], 1024, 1),
            // A found reference supplies the size; the 65536 that follows is not a size field.
            inter_frame(false, &[0, 1], 65536, 65536),
            bits(&[(2, 2), (0, 1), (0, 1), (1, 1), (3, 3)]),
        ];
        for header in ok {
            check_header(&header).unwrap();
        }
    }

    #[test]
    fn header_sizes_beyond_limit_fail() {
        let too_big = [
            key_frame(2000, 100),
            key_frame(1, 1025),
            intra_only_frame(1025, 1),
            inter_frame(false, &[0, 0, 0], 1, 2000),
            inter_frame(true, &[0, 0, 0], 4096, 4096),
        ];
        for header in too_big {
            let err = check_header(&header).unwrap_err();
            assert!(err.to_string().contains("1024"), "{err}");
        }
    }

    #[test]
    fn unsupported_headers_fail() {
        let profile_2 = bits(&[(2, 2), (0, 1), (1, 1), (0, 1), (0, 1), (1, 1), (0, 1), SYNC]);
        let err = check_header(&profile_2).unwrap_err();
        assert!(err.to_string().contains("profile 2"), "{err}");
        let bad_marker = bits(&[(1, 2), (0, 6)]);
        assert!(check_header(&bad_marker).is_err());
        let mut bad_sync = key_frame(100, 100);
        bad_sync[1] ^= 0xFF;
        assert!(check_header(&bad_sync).is_err());
        let rgb = bits(&[
            (2, 2),
            (0, 4),
            (1, 1),
            (0, 1),
            SYNC,
            (7, 3),
            (99, 16),
            (99, 16),
        ]);
        assert!(check_header(&rgb).is_err());
    }

    #[test]
    fn decoding_stops_at_the_byte_budget() {
        let packets = demux(ALPHA_WEBM).unwrap();
        let needed = 60 * (100 * 100 * 2 + 50 * 50 * 2 + FRAME_OVERHEAD);
        assert_eq!(decode_packets(&packets, needed).unwrap().len(), 60);
        let err = decode_packets(&packets, needed - 1).unwrap_err();
        assert!(err.to_string().contains("limit"), "{err}");
    }

    #[test]
    fn missing_alpha_is_opaque() {
        let yuva = to_yuva(frame(3, 2), None).unwrap();
        assert_eq!((yuva.w, yuva.h), (3, 2));
        assert_eq!(yuva.y, [50; 6]);
        assert_eq!((yuva.u, yuva.v), (vec![60; 2], vec![70; 2]));
        assert_eq!(yuva.a, [255; 6]);
    }

    #[test]
    fn alpha_comes_from_the_alpha_luma() {
        let mut alpha = frame(3, 2);
        alpha.y = PlaneData::U8(vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(
            to_yuva(frame(3, 2), Some(alpha)).unwrap().a,
            [0, 1, 2, 3, 4, 5]
        );
    }

    #[test]
    fn mismatched_alpha_is_error() {
        assert!(to_yuva(frame(4, 2), Some(frame(2, 2))).is_err());
    }

    #[test]
    fn non_420_8_bit_frames_are_errors() {
        let mut deep = frame(2, 2);
        deep.bit_depth = 10;
        deep.y = PlaneData::U16(vec![512; 4]);
        assert!(to_yuva(deep, None).is_err());
        let mut full_chroma = frame(2, 2);
        (full_chroma.subsampling_x, full_chroma.subsampling_y) = (0, 0);
        full_chroma.u = PlaneData::U8(vec![60; 4]);
        full_chroma.v = PlaneData::U8(vec![70; 4]);
        assert!(to_yuva(full_chroma, None).is_err());
        let mut short_plane = frame(2, 2);
        short_plane.y = PlaneData::U8(vec![50; 3]);
        assert!(to_yuva(short_plane, None).is_err());
        assert!(to_yuva(frame(0, 0), None).is_err());
    }
}
