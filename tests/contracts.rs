use clap::Parser;
use ripmcp::cli::Cli;
use ripmcp::deadline::{Deadline, Timeouts};
use ripmcp::error::ErrorKind;
use serde_json::Value;
use std::time::Duration;

#[test]
fn timeout_defaults_and_global_override() {
    let cases: &[(&[&str], u64)] = &[
        (&["servers"], 60),
        (&["auth", "login", "s"], 300),
        (&["auth", "status", "s"], 60),
        (&["auth", "logout", "s"], 60),
        (&["--timeout", "7", "auth", "login", "s"], 7),
        (&["auth", "login", "s", "--timeout", "7"], 7),
        (&["--uninstall-everything", "--timeout", "7"], 7),
    ];
    for (args, seconds) in cases {
        let cli: Cli =
            Cli::try_parse_from(std::iter::once("ripmcp").chain(args.iter().copied())).unwrap();
        assert_eq!(cli.timeout_duration(), Duration::from_secs(*seconds));
    }
}

#[test]
fn timeout_configuration_is_strict_and_cli_takes_precedence() {
    let configured: Timeouts =
        serde_json::from_str(r#"{"operation_seconds":12,"login_seconds":34}"#).unwrap();
    let defaulted: Timeouts = serde_json::from_str("{}").unwrap();
    assert_eq!(defaulted.operation_seconds.get(), 60);
    assert_eq!(defaulted.login_seconds.get(), 300);
    let cases: &[(&[&str], u64)] = &[
        (&["servers"], 12),
        (&["auth", "login", "s"], 34),
        (&["servers", "--timeout", "56"], 56),
    ];
    for (args, seconds) in cases {
        let cli: Cli =
            Cli::try_parse_from(std::iter::once("ripmcp").chain(args.iter().copied())).unwrap();
        assert_eq!(cli.timeout_with(&configured), Duration::from_secs(*seconds));
    }
    for raw in [
        r#"{"operation_seconds":0}"#,
        r#"{"login_seconds":-1}"#,
        r#"{"login_seconds":0.5}"#,
        r#"{"unknown":1}"#,
        r#"{"login_seconds":null}"#,
    ] {
        assert!(serde_json::from_str::<Timeouts>(raw).is_err());
    }
}

#[test]
fn deadlines_expire_without_overflow_or_reset() {
    let expired: Deadline = Deadline::new(Duration::ZERO);
    assert_eq!(expired.remaining().unwrap_err().kind, ErrorKind::Timeout);
    let deadline: Deadline = Deadline::new(Duration::from_secs(u64::MAX));
    let first: Duration = deadline.remaining().unwrap();
    assert!(deadline.remaining().unwrap() <= first);
}

#[test]
fn error_codes_are_stable() {
    let kinds: [ErrorKind; 11] = [
        ErrorKind::Io,
        ErrorKind::Usage,
        ErrorKind::Configuration,
        ErrorKind::Connection,
        ErrorKind::Protocol,
        ErrorKind::Authentication,
        ErrorKind::ToolResult,
        ErrorKind::PartialFailure,
        ErrorKind::Timeout,
        ErrorKind::Unsupported,
        ErrorKind::Cancelled,
    ];
    for (kind, code) in kinds.into_iter().zip([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 130]) {
        assert_eq!(kind as u8, code);
    }
}

#[test]
fn tool_failures_keep_the_complete_envelope() {
    for failed in [false, true] {
        let envelope: Value = serde_json::json!({
            "content": [{"type": "text", "text": "not a diagnostic"}],
            "structuredContent": {"nested": [1, null, true]},
            "isError": failed, "_meta": {"extension": "preserved"}
        });
        let mut bytes: Vec<u8> = Vec::new();
        let result: Result<(), ripmcp::error::Error> =
            ripmcp::output::tool_result(&mut bytes, &envelope);
        assert_eq!(
            result.err().map(|error| error.kind),
            failed.then_some(ErrorKind::ToolResult)
        );
        assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), envelope);
    }
}

#[test]
fn invalid_envelope_writes_nothing_and_io_failure_takes_precedence() {
    for envelope in [
        serde_json::json!([]),
        serde_json::json!({"isError": "true"}),
    ] {
        let mut bytes: Vec<u8> = Vec::new();
        assert_eq!(
            ripmcp::output::tool_result(&mut bytes, &envelope)
                .unwrap_err()
                .kind,
            ErrorKind::Protocol
        );
        assert!(bytes.is_empty());
    }
    let mut full: &mut [u8] = &mut [];
    assert_eq!(
        ripmcp::output::tool_result(&mut full, &serde_json::json!({"isError": true}))
            .unwrap_err()
            .kind,
        ErrorKind::Io
    );
}
