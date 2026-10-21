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

    //fusa:req REQ-AVTP-024
    let _ = rcp::avtp::decode_tscf_frame(data);

    //fusa:req REQ-HVSEL-005
    // select_header_variant is exercised under both TimeSyncCapability
    // outcomes and both TSCF fallback policies, since the rule branches on
    // them before decoding the body.
    for ts in [
        rcp::avtp::TimeSyncCapability::Capable,
        rcp::avtp::TimeSyncCapability::Incapable,
    ] {
        for policy in [rcp::avtp::TscfFallback::Drop, rcp::avtp::TscfFallback::Ignore] {
            let _ = rcp::avtp::select_header_variant(data, ts, policy);
        }
    }

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
