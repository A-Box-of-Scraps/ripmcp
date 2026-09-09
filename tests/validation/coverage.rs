use clap::{CommandFactory, Parser};
use ripmcp::cli::Cli;
use std::collections::BTreeSet;

const FORMS: &[&[&str]] = &[
    &["install", "s", "--npx", "p"],
    &["install", "s", "--uvx", "p", "--", "literal argument"],
    &["install", "s", "--docker", "p", "--skip-verify"],
    &["install", "s", "--config", "native.json"],
    &["uninstall", "s"],
    &["uninstall", "s", "--clean"],
    &["uninstall", "s", "--clean", "-y"],
    &["enable", "s"],
    &["disable", "s"],
    &["enable", "s", "t"],
    &["disable", "s", "t"],
    &["start", "s"],
    &["stop", "s"],
    &["servers"],
    &["tools"],
    &["tools", "s"],
    &["tools", "s", "--all"],
    &["tool", "s", "t"],
    &["call", "s", "t", "{}"],
    &["call", "s", "t", "--input", "args.json"],
    &["call", "s", "t", "--input", "-"],
    &["call", "t", "{}"],
    &["call", "t", "--input", "args.json"],
    &["call", "t", "--input", "-"],
    &["shape", "result.json"],
    &["shape", "-", "--depth", "64", "--width", "10000"],
    &["auth", "login", "s"],
    &["auth", "status", "s"],
    &["auth", "logout", "s"],
    &["trust"],
    &["--uninstall-everything"],
    &["--uninstall-everything", "-y"],
];

#[test]
fn every_public_command_form_has_exact_scope_and_timeout_grammar() {
    for form in FORMS {
        for scope in [None, Some("--user"), Some("--project")] {
            let mut args: Vec<&str> = vec!["ripmcp", "--timeout", "7", form[0]];
            if let Some(scope) = scope {
                args.push(scope);
            }
            args.extend_from_slice(&form[1..]);
            let accepts_scope = matches!(form[0], "install" | "uninstall" | "enable" | "disable");
            assert_eq!(
                Cli::try_parse_from(&args).is_ok(),
                scope.is_none() || accepts_scope,
                "{args:?}"
            );
        }
    }
}

#[test]
fn command_matrix_cannot_silently_omit_a_new_public_subcommand() {
    let command: clap::Command = Cli::command();
    let public: BTreeSet<&str> = command
        .get_subcommands()
        .filter(|command| !command.is_hide_set())
        .map(clap::Command::get_name)
        .collect();
    let covered: BTreeSet<&str> = FORMS
        .iter()
        .map(|form| form[0])
        .filter(|name| !name.starts_with('-'))
        .collect();
    assert_eq!(public, covered);
}
