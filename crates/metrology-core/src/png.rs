//! Minimal lossless PNG writer (8-bit grayscale, stored/uncompressed deflate blocks).
//! Used to save averaged snapshots and overlays without an image dependency.

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const MAX_DIM: usize = i32::MAX as usize; // PNG limit: 2^31 - 1 per dimension
const STORED_BLOCK: usize = 65535;

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = CRC_TABLE[((crc ^ u32::from(b)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += u32::from(x);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

/// Start a chunk: writes length and type; returns the offset of the type for the CRC.
fn begin_chunk(out: &mut Vec<u8>, kind: &[u8; 4], len: u32) -> usize {
    out.extend_from_slice(&len.to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    start
}

/// Finish a chunk: CRC over type + data, computed in place (no copy).
fn end_chunk(out: &mut Vec<u8>, start: usize) {
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Encode `pixels` (row-major, width*height bytes) as a grayscale PNG.
pub fn encode_gray8(width: usize, height: usize, pixels: &[u8]) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 || width > MAX_DIM || height > MAX_DIM {
        return Err(format!("unsupported size {width}x{height}"));
    }
    let n = width.checked_mul(height).ok_or("image too large")?;
    if pixels.len() != n {
        return Err(format!("{} pixels for {width}x{height}", pixels.len()));
    }
    // Each scanline is prefixed with filter type 0 (None).
    let raw_len = n.checked_add(height).ok_or("image too large")?;
    let blocks = raw_len.div_ceil(STORED_BLOCK);
    let z_len = 2 + blocks * 5 + raw_len + 4;
    let idat_len = u32::try_from(z_len).map_err(|_| "image too large for one IDAT chunk")?;

    let mut raw = Vec::with_capacity(raw_len);
    for row in pixels.chunks(width) {
        raw.push(0);
        raw.extend_from_slice(row);
    }

    let mut out = Vec::with_capacity(8 + 25 + 12 + z_len + 12);
    out.extend_from_slice(&SIGNATURE);

    let c = begin_chunk(&mut out, b"IHDR", 13);
    out.extend_from_slice(&(width as u32).to_be_bytes());
    out.extend_from_slice(&(height as u32).to_be_bytes());
    out.extend_from_slice(&[8, 0, 0, 0, 0]); // 8-bit, grayscale, deflate, filter 0, no interlace
    end_chunk(&mut out, c);

    let c = begin_chunk(&mut out, b"IDAT", idat_len);
    out.extend_from_slice(&[0x78, 0x01]); // zlib header, no compression
    let mut parts = raw.chunks(STORED_BLOCK).peekable();
    while let Some(block) = parts.next() {
        out.push(u8::from(parts.peek().is_none())); // BFINAL, BTYPE=00 (stored)
        out.extend_from_slice(&(block.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(&raw).to_be_bytes());
    end_chunk(&mut out, c);

    let c = begin_chunk(&mut out, b"IEND", 0);
    end_chunk(&mut out, c);
    Ok(out)
}

/// Decode a PNG produced by `encode_gray8` (8-bit grayscale, stored deflate blocks, filter 0
/// on every row). Anything else is rejected; this is not a general PNG reader. Returns
/// (width, height, pixels).
pub fn decode_gray8(png: &[u8]) -> Result<(usize, usize, Vec<u8>), String> {
    if png.len() < 8 || png[..8] != SIGNATURE {
        return Err("not a PNG".into());
    }
    let (mut i, mut ihdr, mut idat) = (8usize, None, Vec::new());
    while i + 12 <= png.len() {
        let len = u32::from_be_bytes(png[i..i + 4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = png[i + 4..i + 8].try_into().unwrap();
        let end = i + 8 + len;
        if end + 4 > png.len() {
            return Err("truncated chunk".into());
        }
        if crc32(&png[i + 4..end]) != u32::from_be_bytes(png[end..end + 4].try_into().unwrap()) {
            return Err(format!("bad CRC in {}", String::from_utf8_lossy(&kind)));
        }
        match &kind {
            b"IHDR" => ihdr = Some(png[i + 8..end].to_vec()),
            b"IDAT" => idat.extend_from_slice(&png[i + 8..end]),
            _ => {}
        }
        i = end + 4;
    }
    let ihdr = ihdr.ok_or("missing IHDR")?;
    if ihdr.len() != 13 || ihdr[8..] != [8, 0, 0, 0, 0] {
        return Err("only 8-bit grayscale, non-interlaced PNGs are supported".into());
    }
    let (w, h) = (
        u32::from_be_bytes(ihdr[0..4].try_into().unwrap()) as usize,
        u32::from_be_bytes(ihdr[4..8].try_into().unwrap()) as usize,
    );
    if idat.len() < 6 || idat[0] != 0x78 {
        return Err("bad zlib header".into());
    }
    let expected = w
        .checked_add(1)
        .and_then(|r| r.checked_mul(h))
        .ok_or("image size overflows")?;
    if expected > 1 << 31 {
        return Err("image too large".into());
    }
    let (mut j, mut raw) = (2usize, Vec::with_capacity(expected));
    loop {
        if j + 5 > idat.len() {
            return Err("truncated deflate stream".into());
        }
        let (hdr, len) = (
            idat[j],
            u16::from_le_bytes([idat[j + 1], idat[j + 2]]) as usize,
        );
        if hdr > 1 || u16::from_le_bytes([idat[j + 3], idat[j + 4]]) != !(len as u16) {
            return Err("not a stored deflate block".into());
        }
        raw.extend_from_slice(idat.get(j + 5..j + 5 + len).ok_or("truncated block")?);
        j += 5 + len;
        if hdr == 1 {
            break;
        }
    }
    if idat
        .get(j..j + 4)
        .map(|a| u32::from_be_bytes(a.try_into().unwrap()))
        != Some(adler32(&raw))
    {
        return Err("bad adler32".into());
    }
    if raw.len() != expected {
        return Err("size mismatch".into());
    }
    let mut pixels = Vec::with_capacity(w * h);
    for row in raw.chunks(w + 1) {
        if row[0] != 0 {
            return Err("only filter type 0 is supported".into());
        }
        pixels.extend_from_slice(&row[1..]);
    }
    Ok((w, h, pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse chunks, verifying every CRC; returns (type, data) pairs.
    fn chunks(png: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
        assert_eq!(&png[..8], &SIGNATURE);
        let (mut i, mut out) = (8, Vec::new());
        while i < png.len() {
            let len = u32::from_be_bytes(png[i..i + 4].try_into().unwrap()) as usize;
            let kind: [u8; 4] = png[i + 4..i + 8].try_into().unwrap();
            let data = png[i + 8..i + 8 + len].to_vec();
            let crc = u32::from_be_bytes(png[i + 8 + len..i + 12 + len].try_into().unwrap());
            assert_eq!(crc, crc32(&png[i + 4..i + 8 + len]), "crc of {kind:?}");
            out.push((kind, data));
            i += 12 + len;
        }
        out
    }

    /// Inflate a zlib stream made only of stored blocks, checking every header field.
    /// Returns (payload, final block length).
    fn inflate_stored(z: &[u8]) -> (Vec<u8>, usize) {
        assert_eq!(&z[..2], &[0x78, 0x01]);
        assert_eq!(((u32::from(z[0]) << 8) | u32::from(z[1])) % 31, 0, "FCHECK");
        let (mut i, mut payload) = (2, Vec::new());
        loop {
            let (hdr, len) = (z[i], u16::from_le_bytes([z[i + 1], z[i + 2]]) as usize);
            let nlen = u16::from_le_bytes([z[i + 3], z[i + 4]]);
            assert_eq!(nlen, !(len as u16), "NLEN is the complement of LEN");
            assert!(hdr == 0 || hdr == 1, "stored block header {hdr}");
            payload.extend_from_slice(&z[i + 5..i + 5 + len]);
            i += 5 + len;
            if hdr == 1 {
                let adler = u32::from_be_bytes(z[i..i + 4].try_into().unwrap());
                assert_eq!(adler, adler32(&payload), "adler32 trailer");
                assert_eq!(i + 4, z.len(), "no trailing bytes");
                return (payload, len);
            }
        }
    }

    #[test]
    fn crc32_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn adler32_standard_check_value() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    /// Exercises the 5552-byte modulo chunking against a straightforward u64 computation.
    #[test]
    fn adler32_matches_naive_over_many_chunks() {
        let data: Vec<u8> = (0..100_000u32).map(|i| (i * 31 % 256) as u8).collect();
        let (mut a, mut b) = (1u64, 0u64);
        for &x in &data {
            a = (a + u64::from(x)) % 65521;
            b = (b + a) % 65521;
        }
        assert_eq!(adler32(&data), ((b << 16) | a) as u32);
    }

    #[test]
    fn iend_chunk_has_its_known_crc() {
        let png = encode_gray8(1, 1, &[7]).unwrap();
        assert_eq!(
            &png[png.len() - 12..],
            &[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]
        );
    }

    /// Full round trip: parse our own file with every CRC checked, inflate it, strip the
    /// filter bytes and compare with the input. 300x250 => raw 75250 bytes => two blocks.
    #[test]
    fn round_trip_reproduces_every_pixel() {
        let (w, h) = (300usize, 250usize);
        let px: Vec<u8> = (0..w * h).map(|i| (i * 7 % 251) as u8).collect();
        let png = encode_gray8(w, h, &px).unwrap();
        let cs = chunks(&png);
        assert_eq!(
            cs.iter().map(|c| &c.0).collect::<Vec<_>>(),
            [b"IHDR", b"IDAT", b"IEND"]
        );
        let ihdr = &cs[0].1;
        assert_eq!(u32::from_be_bytes(ihdr[0..4].try_into().unwrap()), 300);
        assert_eq!(u32::from_be_bytes(ihdr[4..8].try_into().unwrap()), 250);
        assert_eq!(&ihdr[8..], &[8, 0, 0, 0, 0]);
        let (raw, last_len) = inflate_stored(&cs[1].1);
        assert_eq!(raw.len(), 250 * 301);
        assert_eq!(last_len, 75250 - 65535, "final block carries the remainder");
        let mut back = Vec::new();
        for row in raw.chunks(301) {
            assert_eq!(row[0], 0, "filter type None");
            back.extend_from_slice(&row[1..]);
        }
        assert_eq!(back, px);
    }

    #[test]
    fn single_block_image_is_marked_final() {
        let cs = chunks(&encode_gray8(4, 3, &[5; 12]).unwrap());
        let (raw, last) = inflate_stored(&cs[1].1);
        assert_eq!((raw.len(), last), (15, 15));
    }

    #[test]
    fn bad_inputs_are_rejected() {
        assert!(encode_gray8(2, 2, &[0; 3]).is_err());
        assert!(encode_gray8(0, 2, &[]).is_err());
        assert!(encode_gray8(usize::MAX, 2, &[]).is_err());
        assert!(encode_gray8(i32::MAX as usize + 1, 1, &[]).is_err());
    }

    #[test]
    fn decode_reproduces_what_encode_wrote() {
        let (w, h) = (300usize, 250usize);
        let px: Vec<u8> = (0..w * h).map(|i| (i * 13 % 253) as u8).collect();
        let png = encode_gray8(w, h, &px).unwrap();
        assert_eq!(decode_gray8(&png).unwrap(), (w, h, px));
    }

    #[test]
    fn decode_rejects_corruption_and_foreign_files() {
        let png = encode_gray8(20, 10, &[7u8; 200]).unwrap();
        assert!(decode_gray8(b"not a png at all").is_err());
        let mut bad_crc = png.clone();
        bad_crc[20] ^= 0xFF; // inside IHDR data
        assert!(decode_gray8(&bad_crc).unwrap_err().contains("CRC"));
        let mut bad_data = png.clone();
        let at = png.windows(4).position(|x| x == b"IDAT").unwrap() + 20;
        bad_data[at] ^= 0x55; // pixel byte: IDAT CRC no longer matches
        assert!(decode_gray8(&bad_data).is_err());
        assert!(decode_gray8(&png[..png.len() - 20]).is_err(), "truncated");
    }
}
