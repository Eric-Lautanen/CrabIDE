#![no_main]

use libfuzzer_sys::fuzz_target;

/// Simulate LSP Content-Length frame parsing.
///
/// The fuzzer provides raw bytes that simulate reading from the LSP server's
/// stdout. We attempt to parse the Content-Length header and JSON body just
/// like the real `run_reader` function does.
///
/// Unlike the real transport, this is synchronous and single-shot — it parses
/// one frame from the input and returns. This ensures the parser handles
/// arbitrary malformed input without panicking.
fn parse_lsp_frame(data: &[u8]) {
    // Treat the entire input as the byte stream after the transport has
    // already read the Content-Length header. We look for "\r\n\r\n" to
    // split headers from body, then parse the body as JSON-RPC.
    //
    // This simulates the core parsing done by `run_reader` without needing
    // async I/O.

    // Try to find the header/body boundary.
    let mut content_length: Option<usize> = None;
    let mut pos = 0;

    // Scan for headers (lines ending with \n)
    loop {
        if pos >= data.len() {
            return; // Incomplete input
        }

        // Find next newline
        let mut newline_pos = pos;
        while newline_pos < data.len() && data[newline_pos] != b'\n' {
            newline_pos += 1;
        }

        if newline_pos >= data.len() {
            return; // No newline found
        }

        let line = &data[pos..newline_pos];
        let trimmed = line.trim_ascii();

        if trimmed.is_empty() {
            // End of headers — body starts after '\n'
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
        None => return, // No Content-Length header
    };

    // Check we have enough body bytes
    if pos + length > data.len() {
        return; // Truncated body
    }

    let body = &data[pos..pos + length];

    // Parse JSON — this is the critical part
    let _msg: Result<crabide_lsp::transport::JsonRpcMessage, _> = serde_json::from_slice(body);
}

fuzz_target!(|data: &[u8]| {
    parse_lsp_frame(data);
});
