#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // fusa:req REQ-WIRE-008
    // fusa:req REQ-WIRE-009
    let _ = rcp::wire::decode_command(data);
    let _ = rcp::wire::decode_response(data);
    let _ = rcp::wire::decode_status(data);
});
