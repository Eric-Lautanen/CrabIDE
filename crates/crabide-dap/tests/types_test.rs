//! Unit tests for `crabide-dap`.

use crabide_dap::resolve_adapter;
use crabide_dap::types::*;

// ── DapMessage ────────────────────────────────────────────────────────────────

#[test]
fn dap_message_request() {
    let msg = DapMessage::request(1, "initialize", serde_json::json!({"clientId": "test"}));
    assert_eq!(msg.seq, 1);
    assert_eq!(msg.msg_type, "request");
    assert_eq!(msg.command.as_deref(), Some("initialize"));
    assert!(msg.arguments.is_some());
    assert!(msg.request_seq.is_none());
    assert!(msg.success.is_none());
    assert!(msg.body.is_none());
    assert!(msg.message.is_none());
    assert!(msg.event.is_none());
}

#[test]
fn dap_message_is_response() {
    let resp = DapMessage {
        seq: 2,
        msg_type: "response".into(),
        command: Some("continue".into()),
        arguments: None,
        request_seq: Some(1),
        success: Some(true),
        body: None,
        message: None,
        event: None,
    };
    assert!(resp.is_response());
    assert!(!resp.is_event());

    let event = DapMessage {
        seq: 3,
        msg_type: "event".into(),
        command: None,
        arguments: None,
        request_seq: None,
        success: None,
        body: None,
        message: None,
        event: Some("stopped".into()),
    };
    assert!(!event.is_response());
    assert!(event.is_event());
}

#[test]
fn dap_message_serialize_roundtrip() {
    let msg = DapMessage::request(1, "initialize", serde_json::json!({"clientId": "test"}));
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: DapMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.seq, 1);
    assert_eq!(deserialized.msg_type, "request");
    assert_eq!(deserialized.command.as_deref(), Some("initialize"));
}

#[test]
fn dap_message_serialize_response() {
    let resp = DapMessage {
        seq: 10,
        msg_type: "response".into(),
        command: Some("stackTrace".into()),
        arguments: None,
        request_seq: Some(5),
        success: Some(true),
        body: Some(serde_json::json!({"stackFrames": []})),
        message: None,
        event: None,
    };
    let json = serde_json::to_string(&resp).unwrap();
    let deserialized: DapMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.request_seq, Some(5));
    assert_eq!(deserialized.success, Some(true));
    assert!(deserialized.body.is_some());
}

#[test]
fn dap_message_serialize_error_response() {
    let err_resp = DapMessage {
        seq: 20,
        msg_type: "response".into(),
        command: Some("launch".into()),
        arguments: None,
        request_seq: Some(15),
        success: Some(false),
        body: None,
        message: Some("adapter not found".into()),
        event: None,
    };
    let json = serde_json::to_string(&err_resp).unwrap();
    let deserialized: DapMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.success, Some(false));
    assert_eq!(deserialized.message.as_deref(), Some("adapter not found"));
}

#[test]
fn dap_message_serialize_event() {
    let event = DapMessage {
        seq: 30,
        msg_type: "event".into(),
        command: None,
        arguments: None,
        request_seq: None,
        success: None,
        body: Some(serde_json::json!({"reason": "breakpoint", "threadId": 1})),
        message: None,
        event: Some("stopped".into()),
    };
    let json = serde_json::to_string(&event).unwrap();
    let deserialized: DapMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.event.as_deref(), Some("stopped"));
    assert_eq!(deserialized.msg_type, "event");
}

#[test]
fn dap_message_serialize_omits_optional_fields() {
    let msg = DapMessage::request(1, "initialize", serde_json::json!({}));
    let json = serde_json::to_string(&msg).unwrap();
    // Optional fields should be absent from serialized output.
    assert!(!json.contains("request_seq"));
    assert!(!json.contains("success"));
    assert!(!json.contains("body"));
    assert!(!json.contains("message"));
    assert!(!json.contains("event"));
    assert!(json.contains("command"));
    assert!(json.contains("arguments"));
    assert!(json.contains("\"type\":\"request\""));
}

// ── InitializeRequestArguments ─────────────────────────────────────────────────

#[test]
fn initialize_request_defaults() {
    let args = InitializeRequestArguments::default();
    assert_eq!(args.client_id, "crabide");
    assert_eq!(args.client_name, "crabide Editor");
    assert!(args.lines_start_at1);
    assert!(args.columns_start_at1);
    assert_eq!(args.path_format, "path");
}

#[test]
fn initialize_request_serialize() {
    let args = InitializeRequestArguments::default();
    let json = serde_json::to_value(&args).unwrap();
    assert_eq!(json["clientId"], "crabide");
    assert_eq!(json["linesStartAt1"], true);
    assert_eq!(json["columnsStartAt1"], true);
}

// ── Source ─────────────────────────────────────────────────────────────────────

#[test]
fn source_from_path() {
    let path = std::path::Path::new("/project/src/main.rs");
    let source = Source::from_path(path);
    assert_eq!(source.name.as_deref(), Some("main.rs"));
    assert_eq!(source.path.as_deref(), Some("/project/src/main.rs"));
    assert!(source.source_reference.is_none());
}

#[test]
fn source_from_path_no_name() {
    let path = std::path::Path::new("/");
    let source = Source::from_path(path);
    assert!(source.path.is_some());
}

// ── LaunchRequestArguments ─────────────────────────────────────────────────────

#[test]
fn launch_request_serialize() {
    let args = LaunchRequestArguments {
        stop_on_entry: true,
        program: Some("/bin/echo".into()),
        args: vec!["hello".into(), "world".into()],
        cwd: Some("/tmp".into()),
        env: [("PATH".into(), "/usr/bin".into())].into(),
        extra: [("customField".into(), serde_json::json!(42))].into(),
    };
    let json = serde_json::to_value(&args).unwrap();
    assert_eq!(json["stopOnEntry"], true);
    assert_eq!(json["program"], "/bin/echo");
    assert_eq!(json["args"][0], "hello");
    assert_eq!(json["cwd"], "/tmp");
    assert_eq!(json["env"]["PATH"], "/usr/bin");
    assert_eq!(json["customField"], 42);
}

// ── parse_launch_json ──────────────────────────────────────────────────────────

#[test]
fn parse_launch_json_empty() {
    let configs = parse_launch_json("");
    assert!(configs.is_empty());
}

#[test]
fn parse_launch_json_invalid() {
    let configs = parse_launch_json("not json");
    assert!(configs.is_empty());
}

#[test]
fn parse_launch_json_no_configurations() {
    let configs = parse_launch_json(r#"{"version": "0.2.0"}"#);
    assert!(configs.is_empty());
}

#[test]
fn parse_launch_json_single_config() {
    let json = r#"{
        "version": "0.2.0",
        "configurations": [
            {
                "name": "Debug Program",
                "type": "python",
                "request": "launch",
                "program": "${workspaceFolder}/main.py",
                "args": ["--verbose"],
                "stopOnEntry": true,
                "env": {"PYTHONPATH": "."}
            }
        ]
    }"#;
    let configs = parse_launch_json(json);
    assert_eq!(configs.len(), 1);
    let cfg = &configs[0];
    assert_eq!(cfg.name, "Debug Program");
    assert_eq!(cfg.request, "launch");
    assert_eq!(cfg.program.as_deref(), Some("${workspaceFolder}/main.py"));
    assert_eq!(cfg.args, vec!["--verbose"]);
    assert!(cfg.stop_on_entry);
    assert_eq!(cfg.env.get("PYTHONPATH").map(|s| s.as_str()), Some("."));
    assert_eq!(cfg.adapter_type.as_deref(), Some("python"));
}

#[test]
fn parse_launch_json_attach_request() {
    let json = r#"{
        "configurations": [
            {
                "name": "Attach to Process",
                "type": "node",
                "request": "attach",
                "port": 9229
            }
        ]
    }"#;
    let configs = parse_launch_json(json);
    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0].request, "attach");
    assert_eq!(configs[0].port, Some(9229));
}

#[test]
fn parse_launch_json_extra_fields() {
    let json = r#"{
        "configurations": [
            {
                "name": "Custom",
                "type": "gdb",
                "request": "launch",
                "target": "/bin/app",
                "miDebuggerPath": "/usr/bin/gdb"
            }
        ]
    }"#;
    let configs = parse_launch_json(json);
    assert_eq!(configs.len(), 1);
    assert!(configs[0].extra.contains_key("target"));
    assert!(configs[0].extra.contains_key("miDebuggerPath"));
}

#[test]
fn parse_launch_json_skip_missing_name() {
    let json = r#"{
        "configurations": [{"type": "python", "request": "launch"}]
    }"#;
    let configs = parse_launch_json(json);
    assert!(configs.is_empty());
}

#[test]
fn parse_launch_json_multiple_configs() {
    let json = r#"{
        "configurations": [
            {"name": "A", "type": "python", "request": "launch"},
            {"name": "B", "type": "node", "request": "launch"}
        ]
    }"#;
    let configs = parse_launch_json(json);
    assert_eq!(configs.len(), 2);
    assert_eq!(configs[0].name, "A");
    assert_eq!(configs[1].name, "B");
}

// ── load_launch_configs ────────────────────────────────────────────────────────

#[test]
fn load_launch_configs_nonexistent() {
    let dir = std::env::temp_dir().join("crabide_test_nonexistent");
    let configs = load_launch_configs(&dir);
    assert!(configs.is_empty());
}

// ── LaunchConfig ───────────────────────────────────────────────────────────────

#[test]
fn launch_config_default() {
    let cfg = LaunchConfig::default();
    assert_eq!(cfg.name, "No launch configuration");
    assert_eq!(cfg.request, "launch");
    assert!(cfg.program.is_none());
    assert!(cfg.args.is_empty());
    assert!(cfg.cwd.is_none());
    assert!(cfg.env.is_empty());
    assert!(!cfg.stop_on_entry);
}

// ── Event body types ───────────────────────────────────────────────────────────

#[test]
fn stopped_event_body_serialize() {
    let body = StoppedEventBody {
        reason: "breakpoint".into(),
        description: Some("hit line 42".into()),
        thread_id: Some(1),
        all_threads_stopped: false,
        hit_breakpoint_ids: vec![100, 101],
    };
    let json = serde_json::to_value(&body).unwrap();
    assert_eq!(json["reason"], "breakpoint");
    assert_eq!(json["description"], "hit line 42");
    assert_eq!(json["threadId"], 1);
    assert_eq!(json["hitBreakpointIds"][0], 100);
}

#[test]
fn continued_event_body_serialize() {
    let body = ContinuedEventBody {
        thread_id: 1,
        all_threads_continued: true,
    };
    let json = serde_json::to_value(&body).unwrap();
    assert_eq!(json["threadId"], 1);
    assert_eq!(json["allThreadsContinued"], true);
}

#[test]
fn output_event_body_serialize() {
    let body = OutputEventBody {
        category: Some("stdout".into()),
        output: "Hello, World!\n".into(),
    };
    let json = serde_json::to_value(&body).unwrap();
    assert_eq!(json["category"], "stdout");
    assert_eq!(json["output"], "Hello, World!\n");
}

#[test]
fn breakpoint_event_body_serialize() {
    let body = BreakpointEventBody {
        reason: "new".into(),
        breakpoint: Breakpoint {
            id: Some(42),
            verified: true,
            message: None,
            source: Some(Source {
                name: Some("main.rs".into()),
                path: Some("/project/src/main.rs".into()),
                source_reference: None,
            }),
            line: Some(10),
            column: None,
        },
    };
    let json = serde_json::to_value(&body).unwrap();
    assert_eq!(json["reason"], "new");
    assert_eq!(json["breakpoint"]["id"], 42);
    assert_eq!(json["breakpoint"]["verified"], true);
    assert_eq!(json["breakpoint"]["line"], 10);
}

// ── Breakpoint types ───────────────────────────────────────────────────────────

#[test]
fn source_breakpoint_serialize() {
    let bp = SourceBreakpoint {
        line: 10,
        column: Some(5),
        condition: Some("i > 0".into()),
        hit_condition: Some("5".into()),
        log_message: Some("hit {i}".into()),
    };
    let json = serde_json::to_value(&bp).unwrap();
    assert_eq!(json["line"], 10);
    assert_eq!(json["column"], 5);
    assert_eq!(json["condition"], "i > 0");
}

#[test]
fn breakpoint_serialize() {
    let bp = Breakpoint {
        id: Some(1),
        verified: true,
        message: Some("cleared".into()),
        source: None,
        line: Some(42),
        column: None,
    };
    let json = serde_json::to_value(&bp).unwrap();
    assert_eq!(json["id"], 1);
    assert_eq!(json["verified"], true);
    assert_eq!(json["message"], "cleared");
}

// ── Stack trace types ──────────────────────────────────────────────────────────

#[test]
fn stack_trace_response_serialize() {
    let resp = StackTraceResponse {
        stack_frames: vec![StackFrameInfo {
            id: 100,
            name: "main".into(),
            source: Some(Source {
                name: Some("main.rs".into()),
                path: Some("/src/main.rs".into()),
                source_reference: None,
            }),
            line: 42,
            column: 5,
        }],
        total_frames: Some(1),
    };
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["stackFrames"][0]["id"], 100);
    assert_eq!(json["stackFrames"][0]["name"], "main");
    assert_eq!(json["totalFrames"], 1);
}

#[test]
fn stack_frame_info_serialize() {
    let frame = StackFrameInfo {
        id: 42,
        name: "foo".into(),
        source: None,
        line: 10,
        column: 3,
    };
    let json = serde_json::to_value(&frame).unwrap();
    assert_eq!(json["id"], 42);
    assert_eq!(json["name"], "foo");
}

// ── Scopes and Variables ───────────────────────────────────────────────────────

#[test]
fn scopes_response_serialize() {
    let resp = ScopesResponse {
        scopes: vec![Scope {
            name: "Local".into(),
            variables_reference: 1000,
            expensive: false,
            named_variables: Some(3),
            indexed_variables: None,
        }],
    };
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["scopes"][0]["name"], "Local");
    assert_eq!(json["scopes"][0]["variablesReference"], 1000);
}

#[test]
fn variables_response_serialize() {
    let resp = VariablesResponse {
        variables: vec![VariableInfo {
            name: "x".into(),
            value: "42".into(),
            type_name: Some("int".into()),
            variables_reference: 0,
            named_variables: None,
            indexed_variables: None,
        }],
    };
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["variables"][0]["name"], "x");
    assert_eq!(json["variables"][0]["value"], "42");
}

#[test]
fn variable_info_with_children() {
    let v = VariableInfo {
        name: "obj".into(),
        value: "{...}".into(),
        type_name: Some("Object".into()),
        variables_reference: 500,
        named_variables: Some(2),
        indexed_variables: None,
    };
    let json = serde_json::to_value(&v).unwrap();
    assert_eq!(json["variablesReference"], 500);
    assert_eq!(json["namedVariables"], 2);
}

// ── Disconnect / Thread arguments ─────────────────────────────────────────────

#[test]
fn disconnect_arguments_serialize() {
    let args = DisconnectArguments {
        restart: Some(false),
        terminate_debuggee: Some(true),
    };
    let json = serde_json::to_value(&args).unwrap();
    assert_eq!(json["restart"], false);
    assert_eq!(json["terminateDebuggee"], true);
}

#[test]
fn disconnect_arguments_defaults() {
    let args = DisconnectArguments {
        restart: None,
        terminate_debuggee: None,
    };
    let json = serde_json::to_value(&args).unwrap();
    // Both optional; should be omitted or null.
    // serde skips None fields.
    assert!(!json.as_object().unwrap().contains_key("restart"));
}

// ── resolve_adapter ───────────────────────────────────────────────────────────

#[test]
fn resolve_adapter_explicit_command() {
    let cfg = LaunchConfig {
        adapter_command: "my-dap".into(),
        adapter_args: vec!["--flag".into()],
        ..Default::default()
    };
    let (cmd, args) = resolve_adapter(&cfg);
    assert_eq!(cmd, "my-dap");
    assert_eq!(args, vec!["--flag"]);
}

#[test]
fn resolve_adapter_python() {
    let cfg = LaunchConfig {
        adapter_type: Some("python".into()),
        port: Some(5678),
        ..Default::default()
    };
    let (cmd, args) = resolve_adapter(&cfg);
    assert_eq!(cmd, "debugpy");
    assert!(args.iter().any(|a| a.contains("5678")));
}

#[test]
fn resolve_adapter_debugpy() {
    let cfg = LaunchConfig {
        adapter_type: Some("debugpy".into()),
        ..Default::default()
    };
    let (cmd, _) = resolve_adapter(&cfg);
    assert_eq!(cmd, "debugpy");
}

#[test]
fn resolve_adapter_node() {
    let cfg = LaunchConfig {
        adapter_type: Some("node".into()),
        ..Default::default()
    };
    let (cmd, _) = resolve_adapter(&cfg);
    assert_eq!(cmd, "js-debug-dap");
}

#[test]
fn resolve_adapter_lldb() {
    let cfg = LaunchConfig {
        adapter_type: Some("lldb".into()),
        ..Default::default()
    };
    let (cmd, _) = resolve_adapter(&cfg);
    assert_eq!(cmd, "lldb-dap");
}

#[test]
fn resolve_adapter_gdb() {
    let cfg = LaunchConfig {
        adapter_type: Some("gdb".into()),
        ..Default::default()
    };
    let (cmd, args) = resolve_adapter(&cfg);
    assert_eq!(cmd, "gdb");
    assert_eq!(args, vec!["--interpreter=dap"]);
}

#[test]
fn resolve_adapter_codelldb() {
    let cfg = LaunchConfig {
        adapter_type: Some("codelldb".into()),
        ..Default::default()
    };
    let (cmd, _) = resolve_adapter(&cfg);
    assert_eq!(cmd, "codelldb");
}

#[test]
fn resolve_adapter_unknown_type() {
    let cfg = LaunchConfig {
        adapter_type: Some("unknown-debugger".into()),
        ..Default::default()
    };
    // With no adapter_command and unknown type, should return empty strings.
    let (cmd, args) = resolve_adapter(&cfg);
    assert_eq!(cmd, "");
    assert!(args.is_empty());
}

// ── DAP transport error-path tests ─────────────────────────────────────────────

#[test]
fn dap_message_deserialize_response_with_unknown_fields() {
    // DAP messages may include extra fields — they should be ignored
    let json = r#"{"seq":1,"type":"response","request_seq":1,"success":true,"command":"continue","extraField":"ignored"}"#;
    let msg: DapMessage = serde_json::from_str(json).unwrap();
    assert!(msg.is_response());
    assert_eq!(msg.command.as_deref(), Some("continue"));
}

#[test]
fn dap_message_deserialize_response_missing_request_seq() {
    // Missing request_seq in response (should still parse)
    let json = r#"{"seq":1,"type":"response","success":true,"command":"continue"}"#;
    let msg: DapMessage = serde_json::from_str(json).unwrap();
    assert!(msg.is_response());
    assert!(msg.request_seq.is_none());
}

#[test]
fn dap_message_deserialize_response_with_error_message() {
    let json = r#"{"seq":2,"type":"response","request_seq":1,"success":false,"command":"launch","message":"adapter not found"}"#;
    let msg: DapMessage = serde_json::from_str(json).unwrap();
    assert_eq!(msg.success, Some(false));
    assert_eq!(msg.message.as_deref(), Some("adapter not found"));
}

#[test]
fn dap_message_deserialize_event_with_body() {
    let json = r#"{"seq":3,"type":"event","event":"stopped","body":{"reason":"breakpoint","threadId":1}}"#;
    let msg: DapMessage = serde_json::from_str(json).unwrap();
    assert!(msg.is_event());
    assert_eq!(msg.event.as_deref(), Some("stopped"));
    assert!(msg.body.is_some());
}

#[test]
fn dap_message_deserialize_event_no_body() {
    let json = r#"{"seq":4,"type":"event","event":"terminated"}"#;
    let msg: DapMessage = serde_json::from_str(json).unwrap();
    assert!(msg.is_event());
    assert!(msg.body.is_none());
}

#[test]
fn dap_message_deserialize_missing_type_field() {
    // Missing type defaults to empty string
    let json = r#"{"seq":1}"#;
    let msg: DapMessage = serde_json::from_str(json).unwrap();
    assert_eq!(msg.msg_type, "");
    assert!(!msg.is_response());
    assert!(!msg.is_event());
}

#[test]
fn dap_message_serialize_request_with_all_fields() {
    let msg = DapMessage::request(10, "evaluate", serde_json::json!({"expression": "2+2"}));
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"seq\":10"));
    assert!(json.contains("\"type\":\"request\""));
    assert!(json.contains("\"command\":\"evaluate\""));
    assert!(json.contains("\"arguments\":{\"expression\":\"2+2\"}"));
}

#[test]
fn dap_message_response_roundtrip_with_body() {
    let original = DapMessage {
        seq: 5,
        msg_type: "response".into(),
        command: Some("stackTrace".into()),
        arguments: None,
        request_seq: Some(3),
        success: Some(true),
        body: Some(serde_json::json!({"stackFrames": [{"id":1,"name":"main","line":42,"column":5}]})),
        message: None,
        event: None,
    };
    let json = serde_json::to_string(&original).unwrap();
    let deserialized: DapMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.request_seq, Some(3));
    assert!(deserialized.success.unwrap_or(false));
    assert!(deserialized.body.is_some());
}

#[test]
fn dap_message_response_with_null_body() {
    let json = r#"{"seq":6,"type":"response","request_seq":4,"success":true,"command":"continue","body":null}"#;
    let msg: DapMessage = serde_json::from_str(json).unwrap();
    assert!(msg.is_response());
    // Null body is deserialized as Some(Value::Null)
    assert_eq!(msg.body, Some(serde_json::Value::Null));
}

