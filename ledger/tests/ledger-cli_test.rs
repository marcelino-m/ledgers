use std::fs;

use std::process::{Command, Stdio};

#[test]
fn python_tests() {
    let files = list_test_files("tests/ledger-cli").unwrap();

    for f in files {
        let status = Command::new("python3")
            .arg("tests/ledger-cli/run-test.py")
            .arg("--test")
            .arg(&f)
            .arg("--ledger")
            .arg("../target/debug/ledger")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .expect("\n Can't execute the test");

        if !status.success() {
            eprintln!("Test failed: {}", &f);
            panic!();
        }
    }
}

fn list_test_files(dir: &str) -> std::io::Result<Vec<String>> {
    let mut files = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("test") {
            files.push(path.display().to_string());
        }
    }

    Ok(files)
}
