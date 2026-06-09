#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz JsonRpcMessage deserialization from arbitrary bytes.
    // The parser should handle any input without panicking.
    if let Ok(msg) = serde_json::from_slice::<crabide_lsp::transport::JsonRpcMessage>(data) {
        // Roundtrip: serialize back and ensure it doesn't panic
        let _ = serde_json::to_string(&msg);
    }
});
