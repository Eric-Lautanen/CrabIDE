#![no_main]

use libfuzzer_sys::fuzz_target;

/// Simulate DAP Content-Length frame parsing.
///
/// Same structure as the LSP frame parser, but deserializes `DapMessage`.
fn parse_dap_frame(data: &[u8]) {
    let mut content_length: Option<usize> = None;
    let mut pos = 0;

    loop {
        if pos >= data.len() {
            return;
        }

        let mut newline_pos = pos;
        while newline_pos < data.len() && data[newline_pos] != b'\n' {
            newline_pos += 1;
        }

        if newline_pos >= data.len() {
            return;
        }

        let line = &data[pos..newline_pos];
        let trimmed = line.trim_ascii();

        if trimmed.is_empty() {
            pos = newline_pos + 1;
            break;
        }

        if let Ok(rest) = std::str::from_utf8(trimmed) {
            if let Some(rest) = rest.strip_prefix("Content-Length:") {
                if let Ok(n) = rest.trim().parse::<usize>() {
                    content_length = Some(n);
                }
            }
        }

        pos = newline_pos + 1;
    }

    let length = match content_length {
        Some(n) => n,
        None => return,
    };

    if pos + length > data.len() {
        return;
    }

    let body = &data[pos..pos + length];

    let _msg: Result<crabide_dap::types::DapMessage, _> = serde_json::from_slice(body);
}

fuzz_target!(|data: &[u8]| {
    parse_dap_frame(data);
});
