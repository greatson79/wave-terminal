//! The opt-in home seam is tested through the real CLI and never writes either home.
use serde_json::Value;
use std::process::{Command, Stdio};

#[test]
fn home_seam_real_cli_matches_feature_contract_without_writes() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp = std::env::temp_dir().join(format!(
        "home-seam 최형석 내 팀 폴더-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir(&temp).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cys"))
        .arg("phoenix-identity")
        .env("CYS_HOME", &temp)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let identity: Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected = if cfg!(all(feature = "test-home-seam", debug_assertions)) {
        temp.clone()
    } else {
        dirs::home_dir().unwrap()
    };
    assert_eq!(
        std::path::Path::new(identity["home_dir"].as_str().unwrap()),
        expected
    );
    assert_eq!(
        std::fs::read_dir(&temp).unwrap().count(),
        0,
        "read-only identity must not write into the override home"
    );
    std::fs::remove_dir(temp).unwrap();
}

#[test]
fn home_seam_test_build_rejects_missing_empty_and_relative_override() {
    for value in [None, Some(""), Some("relative")] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cys"));
        command
            .arg("phoenix-identity")
            .env_remove("CYS_HOME")
            .stdin(Stdio::null());
        if let Some(value) = value {
            command.env("CYS_HOME", value);
        }
        let output = command.output().unwrap();
        if cfg!(all(feature = "test-home-seam", debug_assertions)) {
            assert!(
                !output.status.success(),
                "test build must fail closed instead of using the real profile"
            );
            assert!(String::from_utf8_lossy(&output.stderr)
                .contains("CYS_HOME must be an absolute test path"));
        } else {
            assert!(
                output.status.success(),
                "default/release builds ignore every CYS_HOME value"
            );
            let identity: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                std::path::Path::new(identity["home_dir"].as_str().unwrap()),
                dirs::home_dir().unwrap()
            );
        }
    }
}
