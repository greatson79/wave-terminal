#[test]
fn pack_owner_cli_reports_preserved_files_without_manifest() {
    let base = std::env::temp_dir().join(format!("cys-pack-owner-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let pack = base.join("pack");
    std::fs::create_dir_all(pack.join("hooks")).unwrap();
    std::fs::write(pack.join("hooks/appbuild-gate.sh"), "STUDENT-HOOK").unwrap();
    std::fs::write(pack.join("agents.json"), "STUDENT-AGENTS").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cys"))
        .args(["init-pack", "--no-install-hook"])
        .env("HOME", &base)
        .env("CYS_PACK_DIR", &pack)
        .env("CYS_CONFIG_DIR", base.join("config"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    for rel in ["hooks/appbuild-gate.sh", "agents.json"] {
        assert_eq!(std::fs::read_to_string(pack.join(rel)).unwrap(),
                   if rel == "agents.json" { "STUDENT-AGENTS" } else { "STUDENT-HOOK" });
        assert!(pack.join(format!("{rel}.new")).is_file());
        assert!(stdout.contains(&format!("이 파일은 고치셔서 그대로 두었습니다 · 새 판 {rel}.new")));
    }
    assert!(stdout.contains("합치려면 cys pack-merge"));
    let _ = std::fs::remove_dir_all(base);
}
