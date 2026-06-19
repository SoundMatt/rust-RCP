#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // fusa:req REQ-E2E-003
    // fusa:req REQ-E2E-008
    let _ = rcp::e2e::unwrap(data);
});
