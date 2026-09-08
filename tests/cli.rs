mod support;

use clap::{CommandFactory, Parser};
use ripmcp::call::{Input, Request};
use ripmcp::cli::{Cli, Command};
use std::process::Output;
use support::Sandbox;

#[test]
fn parser_is_consistent() {
    Cli::command().debug_assert();
}

#[test]
fn six_call_forms() {
    for qualified in [false, true] {
        for source in ["inline", "file", "stdin"] {
            let mut args: Vec<&str> = vec!["ripmcp", "call"];
            if qualified {
                args.push("server");
            }
            args.push("tool");
            match source {
                "inline" => args.push("{}"),
                "file" => args.extend(["--input", "arguments.json"]),
                _ => args.extend(["--input", "-"]),
            }
            let cli: Cli = Cli::try_parse_from(args).unwrap();
            let Some(Command::Call(call)): Option<Command> = cli.command else {
                panic!("expected call")
            };
            let request: Request = call.into_request().unwrap();
            assert_eq!(request.server.as_deref(), qualified.then_some("server"));
            assert_eq!(request.tool, "tool");
            assert!(matches!(
                (source, request.input),
                ("inline", Input::Inline(_)) | ("file", Input::File(_)) | ("stdin", Input::Stdin)
            ));
        }
    }
}

#[test]
fn command_forms_have_explicit_unsupported_errors() {
    let sandbox: Sandbox = Sandbox::new();
    let forms: &[&[&str]] = &[
        &["install", "s", "--npx", "pkg", "--", "--flag"],
        &["install", "s", "--uvx", "pkg", "--project"],
        &["install", "s", "--docker", "image", "--skip-verify"],
        &["install", "s", "--config", "missing.json", "--user"],
        &["uninstall", "s"],
        &["uninstall", "s", "--clean", "-y"],
        &["enable", "s"],
        &["disable", "s", "t"],
        &["enable", "s", "t", "--project"],
        &["disable", "s", "--user"],
        &["start", "s"],
        &["stop", "s"],
        &["servers"],
        &["tools"],
        &["tools", "s", "--all"],
        &["tool", "s", "t"],
        &["shape", "-"],
        &["auth", "login", "s"],
        &["auth", "status", "s"],
        &["auth", "logout", "s"],
        &["--uninstall-everything", "-y"],
        &["call", "t", "--input", "missing"],
    ];
    for args in forms {
        let output: Output = sandbox.run(args);
        assert_eq!(output.status.code(), Some(10), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    assert_eq!(std::fs::read_dir(sandbox.root.path()).unwrap().count(), 0);
}

#[test]
fn invalid_arguments_have_no_side_effects_or_secret_echo() {
    let sandbox: Sandbox = Sandbox::new();
    let forms: &[&[&str]] = &[
        &[],
        &["install", "s"],
        &["install", "s", "--npx", "x", "--uvx", "x"],
        &["install", "s", "--npx", "x", "--user", "--project"],
        &["install", "s", "--config", "x", "--", "secret"],
        &["call", "t"],
        &["call", "t", "secret"],
        &["call", "t", "[]"],
        &["call", "s", "t", "{}", "--input", "secret"],
        &["call", "t", "--input", "a", "--input", "secret"],
        &["start"],
        &["tool", "s"],
        &["servers", "secret"],
        &["--uninstall-everything", "servers"],
        &["-y"],
        &["uninstall", "s", "-y"],
        &["shape", "-", "--depth", "0"],
        &["--timeout", "0", "servers"],
        &["--secret"],
    ];
    for args in forms {
        let output: Output = sandbox.run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("secret"));
    }
    assert_eq!(std::fs::read_dir(sandbox.root.path()).unwrap().count(), 0);
}

#[test]
fn help_and_version_are_successful() {
    let sandbox: Sandbox = Sandbox::new();
    for flag in ["--help", "--version"] {
        let output: Output = sandbox.run(&[flag]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(!output.stdout.is_empty());
    }
}

#[test]
fn output_preserves_envelope_and_reports_io_failure() {
    let value: serde_json::Value = serde_json::json!({"content": [], "isError": true});
    let mut bytes: Vec<u8> = Vec::new();
    ripmcp::output::json(&mut bytes, &value).unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        value
    );
    let mut full: &mut [u8] = &mut [];
    assert_eq!(
        ripmcp::output::json(&mut full, &value).unwrap_err().kind,
        ripmcp::error::ErrorKind::Io
    );
}
