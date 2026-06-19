#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // fusa:req REQ-CFG-005
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = rcp::config::from_json(s);
        let _ = rcp::config::from_yaml(s);
    }
});
