//! MPEG-TS muxer for H.264 Annex-B access units (PAT + PMT + PES on PID 0x100).

#![forbid(unsafe_code)]

const TS_PACKET: usize = 188;
const SYNC: u8 = 0x47;
const PID_PAT: u16 = 0x0000;
const PID_PMT: u16 = 0x1000;
const PID_VIDEO: u16 = 0x0100;

/// Continuity-aware MPEG-TS muxer. Each HLS segment should start with PAT/PMT.
#[derive(Debug, Clone)]
pub struct TsMuxer {
    cc_pat: u8,
    cc_pmt: u8,
    cc_video: u8,
}

impl Default for TsMuxer {
    fn default() -> Self {
        Self::new()
    }
}

impl TsMuxer {
    pub fn new() -> Self {
        Self {
            cc_pat: 0,
            cc_pmt: 0,
            cc_video: 0,
        }
    }

    /// Appends PAT and PMT packets (needed at the start of every HLS segment).
    pub fn write_pat_pmt(&mut self, out: &mut Vec<u8>) {
        let mut pat = [0xFFu8; TS_PACKET];
        write_psi_packet(&mut pat, PID_PAT, &pat_section(), &mut self.cc_pat);
        out.extend_from_slice(&pat);

        let mut pmt = [0xFFu8; TS_PACKET];
        write_psi_packet(&mut pmt, PID_PMT, &pmt_section(), &mut self.cc_pmt);
        out.extend_from_slice(&pmt);
    }

    /// Packetizes one H.264 access unit (Annex-B) as a PES on the video PID.
    pub fn write_h264_pes(&mut self, out: &mut Vec<u8>, annex_b: &[u8], pts_us: u64) {
        if annex_b.is_empty() {
            return;
        }
        let pes = build_video_pes(annex_b, pts_us);
        let pts_90k = pts_us_to_90k(pts_us);
        packetize_pes(out, &pes, pts_90k, &mut self.cc_video);
    }
}

/// Builds a rolling MPEG-TS segment from arbitrary codec payload (Annex-B style or raw).
pub fn append_media_to_segment(existing: &mut Vec<u8>, payload: &[u8], pts_us: u64) {
    let mut mux = TsMuxer::new();
    if existing.is_empty() {
        mux.write_pat_pmt(existing);
    } else {
        // Preserve continuity as best-effort from existing packet count.
        let packets = existing.len() / TS_PACKET;
        mux.cc_pat = 0;
        mux.cc_pmt = 1;
        mux.cc_video = packets.saturating_sub(2) as u8;
    }
    mux.write_h264_pes(existing, payload, pts_us);
}

/// Fresh segment with PAT/PMT and one PES chunk.
pub fn media_to_ts_segment(payload: &[u8], pts_us: u64) -> Vec<u8> {
    let mut mux = TsMuxer::new();
    let mut out = Vec::new();
    mux.write_pat_pmt(&mut out);
    mux.write_h264_pes(&mut out, payload, pts_us);
    out
}

/// True when Annex-B (or AVCC) payload contains an IDR NAL (type 5).
pub fn h264_is_keyframe(data: &[u8]) -> bool {
    for nal in iter_nals(data) {
        if nal.is_empty() {
            continue;
        }
        if nal[0] & 0x1F == 5 {
            return true;
        }
    }
    false
}

/// Converts length-prefixed (AVCC) NALs to Annex-B; leaves Annex-B unchanged.
pub fn to_annex_b(data: &[u8]) -> Vec<u8> {
    if data.starts_with(&[0, 0, 0, 1]) || data.starts_with(&[0, 0, 1]) {
        return data.to_vec();
    }
    let mut i = 0;
    let mut out = Vec::with_capacity(data.len() + 8);
    while i + 4 <= data.len() {
        let n = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
        i += 4;
        if n == 0 || i + n > data.len() {
            return data.to_vec();
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&data[i..i + n]);
        i += n;
    }
    if out.is_empty() {
        data.to_vec()
    } else {
        out
    }
}

fn iter_nals(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    // Yield NALs from Annex-B start codes. Also treat 4-byte lengths as AVCC.
    let annex = data.starts_with(&[0, 0, 0, 1]) || data.starts_with(&[0, 0, 1]);
    std::iter::from_fn({
        let mut i = 0;
        move || {
            if annex {
                while i + 3 < data.len() {
                    let start = if data[i..].starts_with(&[0, 0, 0, 1]) {
                        i + 4
                    } else if data[i..].starts_with(&[0, 0, 1]) {
                        i + 3
                    } else {
                        i += 1;
                        continue;
                    };
                    let mut end = start;
                    while end + 3 < data.len()
                        && !data[end..].starts_with(&[0, 0, 0, 1])
                        && !data[end..].starts_with(&[0, 0, 1])
                    {
                        end += 1;
                    }
                    if end + 3 >= data.len() {
                        end = data.len();
                    }
                    i = end;
                    return Some(&data[start..end]);
                }
                None
            } else {
                if i + 4 > data.len() {
                    return None;
                }
                let n = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
                i += 4;
                if i + n > data.len() {
                    return None;
                }
                let nal = &data[i..i + n];
                i += n;
                Some(nal)
            }
        }
    })
}

fn pts_us_to_90k(pts_us: u64) -> u64 {
    pts_us.saturating_mul(90) / 1_000
}

fn mpeg_crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= (b as u32) << 24;
        for _ in 0..8 {
            if crc & 0x8000_0000 != 0 {
                crc = (crc << 1) ^ 0x04C1_1DB7;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

fn pat_section() -> Vec<u8> {
    // table_id + syntax/length filled after body
    let mut s = vec![
        0x00, // table_id
        0xB0, 0x00, // section_syntax + length placeholder
        0x00, 0x01, // transport_stream_id
        0xC1, 0x00, 0x00, // version/current_next, section numbers
        0x00, 0x01, // program_number 1
        0xE0 | ((PID_PMT >> 8) as u8 & 0x1F),
        PID_PMT as u8,
    ];
    let section_len = (s.len() - 3) + 4; // bytes after length field, plus CRC
    s[1] = 0xB0 | ((section_len >> 8) as u8 & 0x0F);
    s[2] = section_len as u8;
    let crc = mpeg_crc32(&s);
    s.extend_from_slice(&crc.to_be_bytes());
    s
}

fn pmt_section() -> Vec<u8> {
    let mut s = vec![
        0x02, // table_id
        0xB0, 0x00,
        0x00, 0x01, // program_number
        0xC1, 0x00, 0x00,
        0xE0 | ((PID_VIDEO >> 8) as u8 & 0x1F),
        PID_VIDEO as u8, // PCR_PID
        0xF0, 0x00,      // program_info_length 0
        0x1B,            // stream_type H.264
        0xE0 | ((PID_VIDEO >> 8) as u8 & 0x1F),
        PID_VIDEO as u8,
        0xF0, 0x00, // ES_info_length
    ];
    let section_len = (s.len() - 3) + 4;
    s[1] = 0xB0 | ((section_len >> 8) as u8 & 0x0F);
    s[2] = section_len as u8;
    let crc = mpeg_crc32(&s);
    s.extend_from_slice(&crc.to_be_bytes());
    s
}

fn write_psi_packet(pkt: &mut [u8], pid: u16, section: &[u8], cc: &mut u8) {
    pkt.fill(0xFF);
    pkt[0] = SYNC;
    pkt[1] = 0x40 | ((pid >> 8) as u8 & 0x1F); // PUSI
    pkt[2] = pid as u8;
    pkt[3] = 0x10 | (*cc & 0x0F);
    *cc = cc.wrapping_add(1) & 0x0F;
    pkt[4] = 0x00; // pointer_field
    let copy = section.len().min(TS_PACKET - 5);
    pkt[5..5 + copy].copy_from_slice(&section[..copy]);
}

fn build_video_pes(payload: &[u8], pts_us: u64) -> Vec<u8> {
    let pts_90k = pts_us_to_90k(pts_us);
    let mut pes = Vec::with_capacity(payload.len() + 19);
    pes.extend_from_slice(&[0x00, 0x00, 0x01, 0xE0]);
    let header_and_payload = 3 + 5 + payload.len(); // flags(3) + PTS(5) + data
    let pes_len = if header_and_payload > 0xFFFF {
        0
    } else {
        header_and_payload
    };
    pes.push(((pes_len >> 8) & 0xFF) as u8);
    pes.push((pes_len & 0xFF) as u8);
    pes.push(0x80); // '10' + no scrambling
    pes.push(0x80); // PTS only
    pes.push(0x05);
    pes.extend_from_slice(&encode_pts(pts_90k));
    pes.extend_from_slice(payload);
    pes
}

fn encode_pts(pts_90k: u64) -> [u8; 5] {
    let pts = pts_90k & 0x1_FFFF_FFFF;
    [
        0x21 | (((pts >> 29) as u8) & 0x0E),
        (pts >> 22) as u8,
        (((pts >> 14) as u8) & 0xFE) | 0x01,
        (pts >> 7) as u8,
        ((pts as u8) << 1) | 0x01,
    ]
}

fn packetize_pes(out: &mut Vec<u8>, pes: &[u8], pcr_90k: u64, cc: &mut u8) {
    let mut offset = 0;
    let mut first = true;
    while offset < pes.len() {
        let mut pkt = [0xFFu8; TS_PACKET];
        pkt[0] = SYNC;
        pkt[1] = if first {
            0x40 | ((PID_VIDEO >> 8) as u8 & 0x1F)
        } else {
            (PID_VIDEO >> 8) as u8 & 0x1F
        };
        pkt[2] = PID_VIDEO as u8;

        let mut payload_off = 4;
        if first {
            // adaptation + payload, PCR
            pkt[3] = 0x30 | (*cc & 0x0F);
            pkt[4] = 7; // flags + 6-byte PCR
            pkt[5] = 0x10; // PCR_flag
            write_pcr(&mut pkt[6..12], pcr_90k);
            payload_off = 12;
        } else {
            pkt[3] = 0x10 | (*cc & 0x0F);
        }
        *cc = cc.wrapping_add(1) & 0x0F;

        let space = TS_PACKET - payload_off;
        let remain = pes.len() - offset;
        if remain < space {
            // stuff via adaptation field so the packet is exactly 188 bytes
            let stuff = space - remain;
            if first {
                // already have adaptation; extend stuffing
                let extra = stuff;
                pkt[4] = (7 + extra) as u8;
                payload_off = 12 + extra;
            } else if stuff > 0 {
                pkt[3] = 0x30 | (pkt[3] & 0x0F);
                pkt[4] = (stuff - 1) as u8;
                if stuff > 1 {
                    pkt[5] = 0x00;
                    for b in pkt.iter_mut().take(4 + stuff).skip(6) {
                        *b = 0xFF;
                    }
                }
                payload_off = 4 + stuff;
            }
        }

        let take = (pes.len() - offset).min(TS_PACKET - payload_off);
        pkt[payload_off..payload_off + take].copy_from_slice(&pes[offset..offset + take]);
        offset += take;
        out.extend_from_slice(&pkt);
        first = false;
    }
}

fn write_pcr(buf: &mut [u8], pcr_90k: u64) {
    let base = pcr_90k & 0x1_FFFF_FFFF;
    // 33-bit base, 6 reserved, 9 ext=0
    buf[0] = (base >> 25) as u8;
    buf[1] = (base >> 17) as u8;
    buf[2] = (base >> 9) as u8;
    buf[3] = (base >> 1) as u8;
    buf[4] = ((base & 1) as u8) << 7 | 0x7E;
    buf[5] = 0x00;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_starts_with_sync_and_pat_pmt() {
        let seg = media_to_ts_segment(&[0, 0, 0, 1, 0x65], 1_000_000);
        assert!(seg.len() >= TS_PACKET * 2);
        assert_eq!(seg.len() % TS_PACKET, 0);
        assert_eq!(seg[0], SYNC);
        assert_eq!(seg[TS_PACKET], SYNC);
        for pkt in seg.chunks(TS_PACKET) {
            assert_eq!(pkt[0], SYNC);
        }
    }

    #[test]
    fn append_grows_segment() {
        let mut seg = Vec::new();
        append_media_to_segment(&mut seg, b"\x00\x00\x00\x01\x65", 0);
        let len = seg.len();
        append_media_to_segment(&mut seg, b"\x00\x00\x00\x01\x41", 40_000);
        assert!(seg.len() > len);
    }

    #[test]
    fn keyframe_detects_idr() {
        assert!(h264_is_keyframe(&[0, 0, 0, 1, 0x65, 0x00]));
        assert!(!h264_is_keyframe(&[0, 0, 0, 1, 0x41, 0x00]));
    }

    #[test]
    fn pat_crc_covers_section() {
        let s = pat_section();
        assert_eq!(mpeg_crc32(&s[..s.len() - 4]), u32::from_be_bytes(s[s.len() - 4..].try_into().unwrap()));
    }
}
