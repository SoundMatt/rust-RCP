#![no_main]
use libfuzzer_sys::fuzz_target;

// Fuzzes the E2E decode path ported from c-RCP (rcp_e2e_unwrap /
// rcp_e2e_unwrap_framed, c-RCP/src/e2e.c:277-368) over arbitrary frames,
// plus the wrap -> unwrap round trip. The first 15 input octets pick the
// CRC inputs (subtype, header octet 1, tu/framing bits, stream_id,
// avtp_timestamp); the rest is the frame.
fuzz_target!(|data: &[u8]| {
    //fusa:req REQ-CRC-002
    let _ = rcp::e2e::crc32_tc18(data);

    if data.len() < 15 {
        return;
    }
    let subtype = data[0];
    let octet1 = data[1];
    let tu = data[2] & 0x01 != 0;
    let is_ntscf = data[2] & 0x02 != 0;
    let mut sid = [0u8; 8];
    sid.copy_from_slice(&data[3..11]);
    let stream_id = u64::from_be_bytes(sid);
    let mut ts = [0u8; 4];
    ts.copy_from_slice(&data[11..15]);
    let avtp_timestamp = u32::from_be_bytes(ts);
    let frame = &data[15..];

    //fusa:req REQ-E2E-007
    let _ = rcp::e2e::unwrap(subtype, octet1, tu, stream_id, avtp_timestamp, frame);
    //fusa:req REQ-E2E-035
    let _ = rcp::e2e::unwrap_framed(stream_id, is_ntscf, octet1, tu, avtp_timestamp, frame);
    //fusa:req REQ-CRC-013
    let _ = rcp::e2e::split_crc_trailer(frame);

    // The round trip needs the `pad` field (octet 2) inside the real
    // prefix, as in c-RCP; any frame holding a full byte_message_info has it.
    //fusa:req REQ-E2E-009
    if frame.len() < rcp::acf::BYTE_MESSAGE_INFO_LEN {
        return;
    }
    if let Ok(wrapped) = rcp::e2e::wrap(subtype, octet1, tu, stream_id, avtp_timestamp, frame) {
        assert_eq!(
            rcp::e2e::unwrap(subtype, octet1, tu, stream_id, avtp_timestamp, &wrapped),
            Ok(frame.to_vec())
        );
    }
});
