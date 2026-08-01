#![no_main]
use libfuzzer_sys::fuzz_target;

// Milestone 1 "### Validation" checklist item: carry forward the
// never-panics-on-arbitrary/truncated-input fuzz-style discipline
// `fuzz_wire_decode.rs` already applies to `wire.rs`, extended to every
// byte-slice-accepting decode function the AVTPDU/ACF wire-format-core work
// added. Each call is `let _ = ...;`, exactly like `fuzz_wire_decode.rs`: the
// only failure mode under test is a panic inside the crate's own decode
// logic, never an assertion in this harness.
fuzz_target!(|data: &[u8]| {
    //fusa:req REQ-NTSCF-005
    //fusa:req REQ-NTSCF-006
    let _ = rcp::avtp::decode_ntscf_header(data);

    //fusa:req REQ-TSCF-005
    //fusa:req REQ-TSCF-006
    let _ = rcp::avtp::decode_tscf_header(data);

    //fusa:req REQ-HVSEL-005
    // select_header_variant is exercised under both TimeSyncCapability
    // outcomes, since the rule branches on it before decoding the body.
    let _ = rcp::avtp::select_header_variant(data, rcp::avtp::TimeSyncCapability::Capable);
    let _ = rcp::avtp::select_header_variant(data, rcp::avtp::TimeSyncCapability::Incapable);

    //fusa:req REQ-BMI-004
    let _ = rcp::acf::decode_byte_message_info(data);

    //fusa:req REQ-ABB-005
    let _ = rcp::acf::decode_acf_abb(data);

    //fusa:req REQ-GBB-005
    let _ = rcp::acf::decode_acf_gbb(data);

    // Belt-and-suspenders: parse_stream_id/StreamId::from_u64 take a plain
    // u64 rather than a byte slice, so they have no truncated-input shape to
    // panic on the way the decoders above do (see avtp.rs's own
    // never-panics test for that argument in full). Deriving a u64 from the
    // leading fuzz bytes still gives them a pass through this harness at
    // negligible cost.
    if data.len() >= 8 {
        let mut raw = [0u8; 8];
        raw.copy_from_slice(&data[..8]);
        let _ = rcp::avtp::StreamId::from_u64(u64::from_be_bytes(raw));
    }
});
