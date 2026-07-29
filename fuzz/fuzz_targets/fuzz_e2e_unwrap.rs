#![no_main]
use libfuzzer_sys::fuzz_target;

// Repointed at the TC18 safe-point CRC-32 (rcp::e2e::crc32_tc18) by
// ROADMAP.md Milestone 9's `e2e` REPLACE cutover, the same way
// `fuzz_wire_decode.rs` was repointed at `avtp::decode_ntscf_frame` by the
// immediately preceding `wire` REPLACE cutover: the legacy CRC-16
// `e2e::unwrap` this target used to fuzz has been deleted outright, and
// `crc32_tc18` is this module's remaining function that runs directly over
// an arbitrary caller-supplied byte slice, matching this harness's own
// `data: &[u8]` shape.
fuzz_target!(|data: &[u8]| {
    // fusa:req REQ-CRC-002
    let _ = rcp::e2e::crc32_tc18(data);
});
