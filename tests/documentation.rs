mod support;

use ripmcp::config::authentication::OAuthClient;
use ripmcp::config::schema::{Configuration, Server};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Output;
use support::Sandbox;

fn documents(directory: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(directory).unwrap() {
        let path: PathBuf = entry.unwrap().path();
        if path.file_name().unwrap() == "old" {
            continue;
        }
        if path.is_dir() {
            paths.extend(documents(&path));
        } else if path.extension().is_some_and(|extension| extension == "md") {
            paths.push(path);
        }
    }
    paths
}

#[test]
fn current_documentation_json_examples_validate_against_native_schemas() {
    let paths: Vec<PathBuf> = documents(&Path::new(env!("CARGO_MANIFEST_DIR")).join("docs"));
    let mut examples = 0;
    for path in paths {
        let source: String = std::fs::read_to_string(&path).unwrap();
        for block in source.split("```json\n").skip(1) {
            let text: &str = block.split_once("\n```").unwrap().0;
            validate_example(text, &path);
            examples += 1;
        }
    }
    assert!(examples > 0);
}

fn validate_example(text: &str, path: &Path) {
    let value: Value = ripmcp::json::parse(text.as_bytes()).unwrap();
    if value.get("client_id").is_some() {
        let client: OAuthClient = serde_json::from_value(value).unwrap();
        assert!(client.validate().is_ok(), "{}", path.display());
        return;
    }
    let configuration: Configuration = if value.get("schema_version").is_some() {
        Configuration::parse(text.as_bytes()).unwrap()
    } else {
        let server: Server = serde_json::from_value(value).unwrap();
        Configuration {
            servers: [("example".to_owned(), server)].into(),
            ..Configuration::default()
        }
    };
    assert!(configuration.validate().is_ok(), "{}", path.display());
}

#[test]
fn documented_protocol_revisions_match_the_client() {
    let paths: Vec<PathBuf> = documents(&Path::new(env!("CARGO_MANIFEST_DIR")).join("docs"));
    let mut revisions = 0;
    for path in paths {
        let source: String = std::fs::read_to_string(&path).unwrap();
        for literal in source.split('`').skip(1).step_by(2) {
            if literal.len() == 10
                && literal
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'-')
            {
                assert_eq!(literal, ripmcp::mcp::PROTOCOL_VERSION, "{}", path.display());
                revisions += 1;
            }
        }
    }
    assert!(revisions > 0);
}

#[test]
fn offline_tutorial_sample_and_shape_commands_match_the_checkpoints() {
    let source: &str = include_str!("../docs/tutorials/inspect-json.md");
    let sample: &str = source
        .split_once("cat > result.json <<'JSON'\n")
        .unwrap()
        .1
        .split_once("\nJSON\n")
        .unwrap()
        .0;
    let sandbox: Sandbox = Sandbox::new();
    std::fs::write(sandbox.root.path().join("result.json"), sample).unwrap();
    for line in source
        .lines()
        .filter(|line| line.starts_with("ripmcp shape result.json"))
    {
        let args: Vec<&str> = line.split_whitespace().skip(1).collect();
        let output: Output = sandbox.run(&args);
        assert!(output.status.success(), "{output:?}");
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        if args.len() == 2 {
            let items: &Value = &report["shape"]["fields"]["structuredContent"]["fields"]["items"];
            assert_eq!(items["length"], 2);
            assert_eq!(items["items"][0]["fields"]["id"]["type"], "number");
            assert_eq!(items["items"][0]["fields"]["title"]["type"], "string");
        } else {
            assert_eq!(report["shape"]["truncated"], true);
            assert!(report["shape"]["omitted"].as_u64().unwrap() > 0);
        }
        assert!(
            !String::from_utf8(output.stdout)
                .unwrap()
                .contains("First item")
        );
    }
    assert_eq!(
        std::fs::read_to_string(sandbox.root.path().join("result.json")).unwrap(),
        sample
    );
}
