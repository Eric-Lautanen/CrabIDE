#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz DapMessage deserialization from arbitrary bytes.
    if let Ok(msg) = serde_json::from_slice::<crabide_dap::types::DapMessage>(data) {
        // Roundtrip: serialize back and ensure it doesn't panic
        let _ = serde_json::to_string(&msg);
    }
});
