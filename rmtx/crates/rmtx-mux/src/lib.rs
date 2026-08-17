//! Minimal MPEG-TS segment builder (PAT + PMT + PES on PID 0x100).

#![forbid(unsafe_code)]

const TS_PACKET: usize = 188;
const SYNC: u8 = 0x47;
const _PID_PAT: u16 = 0x0000;
const PID_PMT: u16 = 0x1000;
const PID_VIDEO: u16 = 0x0100;

/// Builds a rolling MPEG-TS segment from arbitrary codec payload (Annex-B style or raw).
pub fn append_media_to_segment(existing: &mut Vec<u8>, payload: &[u8], pts_us: u64) {
    if existing.is_empty() {
        existing.extend_from_slice(&pat_pmt_prefix());
    }
    existing.extend(packetize_pes(payload, pts_us));
}

/// Fresh segment with PAT/PMT and one PES chunk.
pub fn media_to_ts_segment(payload: &[u8], pts_us: u64) -> Vec<u8> {
    let mut out = pat_pmt_prefix().to_vec();
    out.extend(packetize_pes(payload, pts_us));
    out
}

fn pat_pmt_prefix() -> [u8; TS_PACKET * 2] {
    let mut out = [0u8; TS_PACKET * 2];
    write_pat_packet(&mut out[0..TS_PACKET]);
    write_pmt_packet(&mut out[TS_PACKET..TS_PACKET * 2]);
    out
}

fn write_pat_packet(buf: &mut [u8]) {
    buf.fill(0xFF);
    buf[0] = SYNC;
    buf[1] = 0x40;
    buf[2] = 0x00;
    buf[3] = 0x10;
    buf[4] = 0x00;
    // program 1 -> PMT 0x1000
    buf[5] = 0x00;
    buf[6] = 0x01;
    buf[7] = 0xF0 | ((PID_PMT >> 8) as u8 & 0x1F);
    buf[8] = (PID_PMT & 0xFF) as u8;
    buf[9] = 0x00; // length placeholder
}

fn write_pmt_packet(buf: &mut [u8]) {
    buf.fill(0xFF);
    buf[0] = SYNC;
    buf[1] = 0x40 | ((PID_PMT >> 8) as u8 & 0x1F);
    buf[2] = PID_PMT as u8;
    buf[3] = 0x10;
    buf[4] = 0x00;
    // PCR PID 0x100, stream type 0x1B H264
    buf[5] = 0x00;
    buf[6] = 0x02;
    buf[7] = 0xB0;
    buf[8] = 0x0E;
    buf[9] = 0x00;
    buf[10] = 0x01;
    buf[11] = 0xC1;
    buf[12] = 0x00;
    buf[13] = 0x00;
    buf[14] = 0xE0 | ((PID_VIDEO >> 8) as u8 & 0x1F);
    buf[15] = PID_VIDEO as u8;
    buf[16] = 0xF0;
    buf[17] = 0x00;
    buf[18] = 0x1B;
    buf[19] = 0xE0 | ((PID_VIDEO >> 8) as u8 & 0x1F);
    buf[20] = PID_VIDEO as u8;
    buf[21] = 0xF0;
    buf[22] = 0x00;
}

fn packetize_pes(payload: &[u8], pts_us: u64) -> Vec<u8> {
    let mut pes = Vec::with_capacity(payload.len() + 32);
    pes.extend_from_slice(&[0x00, 0x00, 0x01, 0xE0]);
    let pes_len = (payload.len() + 8).min(0xFFFF);
    pes.push(((pes_len >> 8) & 0xFF) as u8);
    pes.push((pes_len & 0xFF) as u8);
    pes.push(0x80);
    pes.push(0x80);
    pes.push(0x05);
    let pts = pts_to_mpeg(pts_us);
    pes.push(0x21);
    pes.extend_from_slice(&pts);
    pes.extend_from_slice(payload);

    let mut packets = Vec::new();
    let mut offset = 0;
    let mut first = true;
    while offset < pes.len() {
        let mut pkt = [0u8; TS_PACKET];
        pkt.fill(0xFF);
        pkt[0] = SYNC;
        pkt[1] = 0x40 | ((PID_VIDEO >> 8) as u8 & 0x1F);
        pkt[2] = PID_VIDEO as u8;
        pkt[3] = if first { 0x30 } else { 0x10 };
        first = false;
        let header_len = 4;
        let avail = TS_PACKET - header_len;
        let take = (pes.len() - offset).min(avail);
        pkt[header_len..header_len + take].copy_from_slice(&pes[offset..offset + take]);
        offset += take;
        packets.extend_from_slice(&pkt);
    }
    packets
}

fn pts_to_mpeg(pts_us: u64) -> [u8; 5] {
    let pts = pts_us * 9 / 100;
    [
        0x21 | (((pts >> 29) & 0x0E) as u8) | 0x01,
        ((pts >> 22) & 0xFF) as u8,
        (((pts >> 14) & 0xFE) as u8) | 0x01,
        ((pts >> 7) & 0xFF) as u8,
        (((pts << 1) & 0xFE) as u8) | 0x01,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_starts_with_sync_and_pat_pmt() {
        let seg = media_to_ts_segment(&[0, 0, 0, 1, 0x65], 1_000_000);
        assert!(seg.len() >= TS_PACKET * 2);
        assert_eq!(seg[0], SYNC);
        assert_eq!(seg[TS_PACKET], SYNC);
    }

    #[test]
    fn append_grows_segment() {
        let mut seg = Vec::new();
        append_media_to_segment(&mut seg, b"a", 0);
        let len = seg.len();
        append_media_to_segment(&mut seg, b"b", 40_000);
        assert!(seg.len() > len);
    }
}
