#[test]
fn custom_report_and_proposal_are_local_and_leave_merge_pending() {
    let base = std::env::temp_dir().join(format!("cys-custom-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let pack = base.join("pack");
    let local = base.join("local");
    std::fs::create_dir_all(&pack).unwrap();
    std::fs::write(pack.join(".pack-version"), "0.14.10\n").unwrap();
    std::fs::write(pack.join("note.txt"), "mine\npassword=marker\n").unwrap();
    std::fs::write(pack.join("note.txt.new"), "vendor\n").unwrap();
    std::fs::write(pack.join("README.md.user"), "my readme\n").unwrap();
    std::fs::write(pack.join("custom.txt"), "custom\n").unwrap();
    let ledger = r#"{"note.txt":{"kind":"new-pending","side":"note.txt.new","version":"0.14.10"},"README.md":{"kind":"healed","side":"README.md.user","version":"0.14.10"}}"#;
    std::fs::write(pack.join(".merge-pending.json"), ledger).unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_cys"))
            .args(args).env("HOME", &base).env("CYS_PACK_DIR", &pack)
            .env("CYS_LOCAL_DIR", &local).env("CYS_NO_AUTOSTART", "1")
            .output().unwrap()
    };

    let out = run(&["pack-merge", "--file", "note.txt", "--propose"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("비밀/개인정보 의심 1건"));
    assert!(!stdout.contains("password=marker"));
    let proposal = std::fs::read_dir(local.join("proposals")).unwrap().next().unwrap().unwrap().path();
    let patch = std::fs::read_to_string(proposal).unwrap();
    assert!(patch.contains("--- vendor/note.txt") || patch.contains("# diff 도구 부재"));
    assert!(patch.contains("password=marker"));

    let out = run(&["pack-merge", "--file", "README.md", "--propose"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(std::fs::read_dir(local.join("proposals")).unwrap().count(), 2);
    assert_eq!(std::fs::read_to_string(pack.join(".merge-pending.json")).unwrap(), ledger);
    assert_eq!(std::fs::read_to_string(pack.join("note.txt")).unwrap(), "mine\npassword=marker\n");
    assert_eq!(std::fs::read_to_string(pack.join("note.txt.new")).unwrap(), "vendor\n");

    let out = run(&["doctor", "--custom-report"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let report_path = std::fs::read_dir(local.join("reports")).unwrap().next().unwrap().unwrap().path();
    let report = std::fs::read_to_string(report_path).unwrap();
    assert!(report.contains("병합 대기 원장 (2건)"));
    assert!(report.contains("보존본 .user (1건)"));
    assert!(report.contains("병치본 .new (1건)"));
    assert!(report.contains("proposals: 2개 파일"));
    assert!(report.contains("custom.txt"));
    assert!(!report.contains("password=marker"));
    assert_eq!(std::fs::read_to_string(pack.join(".merge-pending.json")).unwrap(), ledger);
    let _ = std::fs::remove_dir_all(base);
}
