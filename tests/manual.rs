use clap::CommandFactory;
use ripmcp::cli::Cli;
use ripmcp::config::schema::{Configuration, Server};
use std::collections::BTreeSet;

const MANUAL: &str = include_str!("../docs/reference/manual.md");

fn public_commands(command: &clap::Command, prefix: &str, names: &mut BTreeSet<String>) {
    for subcommand in command.get_subcommands() {
        if subcommand.is_hide_set() {
            assert!(!MANUAL.contains(subcommand.get_name()));
            continue;
        }
        let name: String = format!("{prefix}{}", subcommand.get_name());
        names.insert(name.clone());
        public_commands(subcommand, &format!("{name} "), names);
    }
}

#[test]
fn manual_covers_exactly_the_public_commands() {
    let mut expected: BTreeSet<String> = BTreeSet::new();
    public_commands(&Cli::command(), "", &mut expected);
    let commands: &str = MANUAL
        .split_once("# COMMANDS\n")
        .unwrap()
        .1
        .split_once("\n# INPUT AND OUTPUT")
        .unwrap()
        .0;
    let documented: BTreeSet<String> = commands
        .lines()
        .filter_map(|line| line.strip_prefix("## ").map(str::to_owned))
        .collect();
    assert_eq!(documented, expected);
}

#[test]
fn manual_json_examples_match_native_schemas() {
    let mut servers = 0;
    let mut configurations = 0;
    for block in MANUAL.split("```json\n").skip(1) {
        let source: &str = block.split_once("\n```").unwrap().0;
        let value: serde_json::Value = serde_json::from_str(source).unwrap();
        if value.get("schema_version").is_some() {
            let _: Configuration = serde_json::from_value(value).unwrap();
            configurations += 1;
        } else {
            let _: Server = serde_json::from_value(value).unwrap();
            servers += 1;
        }
    }
    assert_eq!((servers, configurations), (2, 1));
}

#[test]
fn manual_protocol_revision_matches_client() {
    assert!(MANUAL.contains(&format!("MCP `{}`", ripmcp::mcp::PROTOCOL_VERSION)));
}
