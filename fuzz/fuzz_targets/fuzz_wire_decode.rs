#![no_main]
use libfuzzer_sys::fuzz_target;

// ROADMAP.md Milestone 9 (`wire` REPLACE disposition): `rcp::wire` — the
// legacy 16-byte private frame this target used to fuzz — is deleted; its
// role is absorbed by `rcp::avtp`'s NTSCF frame composition
// (`encode_ntscf_frame`/`decode_ntscf_frame`, itself added by this same
// milestone item) plus the ACF decoders `fuzz_avtpdu_acf_decode.rs` already
// covers. This target is repointed at that composition step rather than
// deleted outright, carrying forward the same never-panics-on-
// arbitrary/truncated-input discipline it always applied, now against
// `decode_ntscf_frame` (which itself delegates the header-decode failure
// modes to `decode_ntscf_header`, already covered by
// `fuzz_avtpdu_acf_decode.rs`) and the payload split it returns, fed onward
// into both ACF decoders. Each call is `let _ = ...;`: the only failure
// mode under test is a panic inside the crate's own decode logic.
fuzz_target!(|data: &[u8]| {
    // fusa:req REQ-WIRE-008
    // fusa:req REQ-WIRE-009
    if let Ok((_hdr, payload)) = rcp::avtp::decode_ntscf_frame(data) {
        let _ = rcp::acf::decode_acf_abb(payload);
        let _ = rcp::acf::decode_acf_gbb(payload);
    }
});
