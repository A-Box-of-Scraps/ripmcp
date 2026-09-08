use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    process::{Child, Command, Output, Stdio},
};
use tempfile::TempDir;

fn shape(bytes: &[u8], args: &[&str]) -> Output {
    let root: TempDir = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".ripmcp")).unwrap();
    fs::write(root.path().join(".ripmcp/config.json"), "invalid config").unwrap();
    let mut child: Child = Command::new(env!("CARGO_BIN_EXE_ripmcp"))
        .env_clear()
        .current_dir(root.path())
        .args(["shape", "-"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output: Output = child.wait_with_output().unwrap();
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    output
}

#[test]
fn offline_shape_reports_heterogeneous_structure_without_scalar_values() {
    let output: Output = shape(
        br#"{"\u00e9":[null,true,42,"secret",{"nested":[]},[1,2]]}"#,
        &[],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("secret"));
    let value: Value = ripmcp::json::parse(&output.stdout).unwrap();
    assert_eq!(
        value["shape"],
        json!({"type":"object","truncated":false,"fields":{"\u{e9}":{"type":"array","length":6,"truncated":false,"items":[{"type":"null"},{"type":"boolean"},{"type":"number"},{"type":"string"},{"type":"object","truncated":false,"fields":{"nested":{"type":"array","length":0,"truncated":false,"items":[]}}},{"type":"array","length":2,"truncated":false,"items":[{"type":"number"},{"type":"number"}]}]}}})
    );
}

#[test]
fn depth_width_and_invalid_inputs_have_explicit_results() {
    let output: Output = shape(
        br#"{"a":{"hidden":42},"b":true,"c":null}"#,
        &["--width", "1", "--depth", "1"],
    );
    assert!(output.status.success());
    let value: Value = ripmcp::json::parse(&output.stdout).unwrap();
    assert_eq!(value["shape"]["omitted"], 2);
    assert_eq!(value["shape"]["fields"]["a"]["omitted"], 1);
    let deep: String = format!("{}0{}", "[".repeat(129), "]".repeat(129));
    for bytes in [
        b"secret".as_slice(),
        b"{} {}",
        b"\xff",
        br#"{"a":1,"a":2}"#,
        deep.as_bytes(),
    ] {
        let output: Output = shape(bytes, &[]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("secret"));
    }
}

#[test]
fn node_budget_bounds_wide_nested_arrays() {
    let value: Value = json!(vec![vec![0; 101]; 1000]);
    let output: Output = shape(&serde_json::to_vec(&value).unwrap(), &["--width", "10000"]);
    assert!(output.status.success());
    let value: Value = ripmcp::json::parse(&output.stdout).unwrap();
    assert_eq!(value["shape"]["truncated"], true);
    assert!(value["shape"]["omitted"].as_u64().unwrap() > 0);
}

#[test]
fn oversized_and_missing_files_have_safe_errors() {
    let root: TempDir = tempfile::tempdir().unwrap();
    let path: std::path::PathBuf = root.path().join("saved.json");
    for expected in [1, 2] {
        if expected == 2 {
            let file: fs::File = fs::File::create(&path).unwrap();
            file.set_len(64 * 1024 * 1024 + 1).unwrap();
        }
        let output: Output = Command::new(env!("CARGO_BIN_EXE_ripmcp"))
            .env_clear()
            .current_dir(root.path())
            .arg("shape")
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(expected));
        assert!(output.stdout.is_empty());
    }
}
