//! Registration/link changes only; the coordinator owns process identity and restoration.
//! Maintenance single-writer scope: 기준_release032rename-r1 22:22; mismatches preserve data and fail visibly.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

fn paths() -> (PathBuf, PathBuf) {
    #[cfg(test)] {
        return TEST_SANDBOX.with(|sandbox| {
            let sandbox = sandbox.borrow();
            let root = &sandbox.as_ref().expect("explicit validated sandbox root required").root;
            (root.join("LaunchAgents/old.plist"), root.join("LaunchAgents/new.plist"))
        });
    }
    #[cfg(not(test))] {
    let new = cys::launchd::plist_path();
    let old = new.with_file_name(format!("{}.plist", cys::launchd::LEGACY_LAUNCHD_LABEL));
    (old, new)
    }
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    #[cfg(test)] { return sandbox_command(program, args); }
    #[cfg(not(test))] {
    let out = Command::new(program).args(args).output()
        .map_err(|e| format!("자동 시작 등록 명령을 실행하지 못했습니다: {e}"))?;
    if !out.status.success() {
        return Err(format!("자동 시작 등록 명령이 실패했습니다: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
    }
}

fn read_exact(path: &Path) -> Result<Option<String>, String> {
    let metadata = match path.symlink_metadata() {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("자동 시작 등록을 읽지 못했습니다: {e}")),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("자동 시작 등록이 일반 파일이 아닙니다. 기존 등록을 유지합니다.".into());
    }
    let mut file = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(path)
        .map_err(|e| format!("자동 시작 등록 안전 열기 실패: {e}"))?;
    let opened = file.metadata().map_err(|e| format!("자동 시작 등록 확인 실패: {e}"))?;
    if !opened.is_file() || (opened.dev(), opened.ino()) != (metadata.dev(), metadata.ino()) {
        return Err("확인 중 자동 시작 등록이 바뀌었습니다.".into());
    }
    let mut contents = String::new();
    file.read_to_string(&mut contents).map_err(|e| format!("자동 시작 등록 보관 실패: {e}"))?;
    let final_meta = path.symlink_metadata().map_err(|e| format!("자동 시작 등록 재확인 실패: {e}"))?;
    if final_meta.file_type().is_symlink() || (final_meta.dev(), final_meta.ino()) != (opened.dev(), opened.ino()) {
        return Err("읽는 중 자동 시작 등록이 바뀌었습니다.".into());
    }
    Ok(Some(contents))
}

fn job(label: &str, path: &Path, expected: &[PathBuf]) -> Result<Value, String> {
    #[cfg(test)] {
        let _ = expected;
        return TEST_SANDBOX.with(|sandbox| {
            let mut sandbox = sandbox.borrow_mut();
            let sandbox = sandbox.as_mut().ok_or("explicit validated sandbox root required")?;
            if label == cys::launchd::LAUNCHD_LABEL && sandbox.foreign_load_after_publish && path.exists() {
                sandbox.foreign_load_after_publish = false;
                sandbox.jobs.insert(label.into(), json!({"kind":"idle","definition":"foreign-canonical"}));
            }
            Ok(sandbox.jobs.get(label).cloned().unwrap_or_else(|| json!({"kind":"absent"})))
        });
    }
    #[cfg(not(test))] {
    let state: Result<Value, String> = match cys::launchd::state_for_label(label) {
        cys::launchd::LaunchdState::Unknown => Err("자동 시작 소유권을 확인하지 못했습니다. 기존 등록을 유지합니다.".into()),
        cys::launchd::LaunchdState::NotLoaded => return Ok(json!({"kind":"absent"})),
        cys::launchd::LaunchdState::LoadedIdle => Ok(json!({"kind":"idle"})),
        cys::launchd::LaunchdState::Running(pid) => {
            let identity = cys::rename_handover::process_identity(pid)?
                .ok_or("등록된 프로세스가 확인 중 종료됐습니다.")?;
            if !expected.contains(&identity.executable) { return Err("다른 실행 파일을 사용하는 등록은 중지하지 않습니다.".into()); }
            Ok(json!({"kind":"running","identity":identity}))
        }
    };
    let state = state?;
    let target = format!("gui/{}/{}", unsafe {libc::getuid()}, label);
    let output = Command::new("/bin/launchctl").args(["print", &target]).output()
        .map_err(|e| format!("적재된 등록 정의 확인 실패: {e}"))?;
    if !output.status.success() { return Err("적재된 등록 정의를 확인하지 못했습니다.".into()); }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| "적재된 등록 응답이 잘못됐습니다.")?;
    let field = |key: &str| text.lines().find_map(|line| line.trim().strip_prefix(key)).map(|v| v.trim().trim_matches('"'));
    if field("path = ") != path.to_str() || !expected.iter().any(|p| field("program = ") == p.to_str()) {
        return Err("적재된 작업이 캡처한 등록 경로 또는 실행 파일과 다릅니다.".into());
    }
    let mut record = state;
    record["definition"] = Value::String(text.into());
    Ok(record)
    }
}

pub(super) fn capture(cli: &Path, daemon: &Path) -> Result<Value, String> {
    let base = daemon.parent().ok_or("앱 실행 파일 경로가 올바르지 않습니다.")?;
    if !cli.is_absolute() || !daemon.is_absolute() || cli.parent() != Some(base) {
        return Err("앱 실행 파일의 절대 경로를 확인하지 못했습니다.".into());
    }
    let expected_old = vec![base.join("cysd"), daemon.into(), PathBuf::from("/opt/homebrew/bin/cysd"), PathBuf::from("/usr/local/bin/cysd")];
    let (old, new) = paths();
    let staged_xml = cys::launchd::render_plist(daemon, &cys::launchd::log_path());
    let old_xml = read_exact(&old)?;
    let new_xml = read_exact(&new)?;
    if old_xml.as_ref().is_some_and(|xml| !expected_old.iter().any(|p|
        *xml == cys::launchd::render_plist(p, &cys::launchd::log_path())
            .replace(cys::launchd::LAUNCHD_LABEL, cys::launchd::LEGACY_LAUNCHD_LABEL)))
        || new_xml.as_ref().is_some_and(|xml| xml != &staged_xml) {
        return Err("사용자 지정 자동 시작 정의를 보존하기 위해 이관을 보류합니다.".into());
    }
    let old_job = job(cys::launchd::LEGACY_LAUNCHD_LABEL, &old, &expected_old)?;
    let new_job = job(cys::launchd::LAUNCHD_LABEL, &new, &[daemon.into()])?;
    let old_loaded = old_job["kind"] != "absent";
    let new_loaded = new_job["kind"] != "absent";
    if (old_loaded && old_xml.is_none()) || (new_loaded && new_xml.is_none()) {
        return Err("실행 중인 자동 시작 등록의 원본 파일이 없어 이관을 보류합니다.".into());
    }
    let links = capture_links(base)?;
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce).map_err(|e| format!("이관 식별자 생성 실패: {e}"))?;
    let attempt_id: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
    let old_inode = inode_at(&old)?;
    let new_inode = inode_at(&new)?;
    Ok(json!({"cli":cli,"daemon":daemon,"old_xml":old_xml,"new_xml":new_xml,
        "attempt_id":attempt_id,"old_inode":old_inode,"new_inode":new_inode,
        "old_loaded":old_loaded,"new_loaded":new_loaded,"old_job":old_job,"new_job":new_job,
        "staged_xml":staged_xml,"links":links,
        "legacy_registration":old_loaded || old_xml.is_some(),
        "canonical_registration":new_loaded && new_xml.is_some()}))
}

fn bin_directories() -> Result<[PathBuf; 2], String> {
    #[cfg(test)] {
        return TEST_SANDBOX.with(|sandbox| {
            let sandbox = sandbox.borrow();
            let root = &sandbox.as_ref().ok_or("explicit validated sandbox root required")?.root;
            Ok([root.join("bin-a"), root.join("bin-b")])
        });
    }
    #[cfg(not(test))] { Ok(["/opt/homebrew/bin".into(), "/usr/local/bin".into()]) }
}

fn capture_links(base: &Path) -> Result<Vec<Value>, String> {
    let mut links = Vec::new();
    for directory in bin_directories()? {
        for name in ["cys", "cysd", "wave", "waved"] {
            let path = directory.join(name);
            let previous = match path.symlink_metadata() {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Ok(meta) if meta.file_type().is_symlink() => {
                    let target = std::fs::read_link(&path).map_err(|e| format!("명령 연결 확인 실패: {e}"))?;
                    let same_kind = if ["cys", "wave"].contains(&name) { ["cys", "wave"] } else { ["cysd", "waved"] };
                    if !same_kind.iter().any(|n| target == base.join(n)) {
                        return Err(format!("사용자 명령 연결을 유지하기 위해 이관을 보류합니다: {}", path.display()));
                    }
                    Some(target)
                }
                Ok(_) => return Err(format!("기존 일반 파일을 덮어쓰지 않습니다: {}", path.display())),
                Err(e) => return Err(format!("명령 연결 확인 실패: {e}")),
            };
            let after = previous.clone().unwrap_or_else(|| base.join(name));
            links.push(json!({"path":path,"before":previous,"after":after}));
        }
    }
    Ok(links)
}

pub(super) fn repair_cli_links(cli: &Path, daemon: &Path) -> Result<(), String> {
    #[cfg(test)]
    let active = TEST_SANDBOX.with(|sandbox| sandbox.borrow().as_ref()
        .map(|s| s.active_handover).ok_or("explicit validated sandbox root required"))?;
    #[cfg(not(test))]
    let active = cys::rename_handover::read_marker()?.is_some();
    if active { return Err("업데이트 복원 중에는 명령 연결을 별도로 변경하지 않습니다.".into()); }
    let base = daemon.parent().ok_or("앱 실행 파일 경로 오류")?;
    if !cli.is_absolute() || !daemon.is_absolute() || cli.parent() != Some(base)
        || cli.file_name().and_then(|n| n.to_str()) != Some("wave")
        || daemon.file_name().and_then(|n| n.to_str()) != Some("waved") {
        return Err("동일 앱의 정식 wave/waved 경로가 필요합니다.".into());
    }
    let captured = capture_links(base)?;
    if captured.iter().all(|link| !link["before"].is_null()) { return Ok(()); }
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce).map_err(|e| format!("명령 연결 식별자 생성 실패: {e}"))?;
    let attempt_id: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
    let mut snapshot = json!({"cli":cli,"daemon":daemon,"attempt_id":attempt_id,"links":captured});
    // shortcut: menu receipts are in memory, revisit only if crash cleanup becomes required.
    match prepare_links(&mut snapshot, &mut |_| Ok(())) {
        Ok(()) => Ok(()),
        Err(error) => match links(&snapshot, "rollback").and_then(|_| links(&snapshot, "cleanup")) {
            Ok(_) => Err(error),
            Err(recovery) => Err(format!("{error}\n명령 연결 복구를 완료하지 못했습니다. 다른 항목은 보존했습니다: {recovery}")),
        },
    }
}

fn quote(value: &str) -> String { format!("'{}'", value.replace('\'', "'\\''")) }

fn link_script(snapshot: &Value, action: &str) -> Result<String, String> {
    let base = Path::new(snapshot["daemon"].as_str().ok_or("명령 연결 기록이 없습니다.")?)
        .parent().ok_or("명령 연결 기록 경로 오류")?;
    let attempt = snapshot["attempt_id"].as_str().ok_or("이관 식별자 오류")?;
    let allowed_directories = bin_directories()?;
    if attempt.len() != 32 || !attempt.bytes().all(|b| b.is_ascii_hexdigit()) { return Err("이관 식별자 오류".into()); }
    let mut script = String::from("set -eu\n");
    if action == "stage" { script.push_str("printf '['\ncomma=''\n"); }
    for (index, link) in snapshot["links"].as_array().ok_or("명령 연결 기록이 없습니다.")?.iter().enumerate() {
        let path = link["path"].as_str().ok_or("명령 연결 경로 오류")?;
        let target = link["after"].as_str().ok_or("명령 연결 대상 오류")?;
        let name = Path::new(path).file_name().and_then(|v| v.to_str()).ok_or("명령 연결 이름 오류")?;
        if !["cys", "cysd", "wave", "waved"].contains(&name)
            || !allowed_directories.iter().any(|directory| Some(directory.as_path()) == Path::new(path).parent())
            {
            return Err("명령 연결 기록이 허용된 경로와 일치하지 않습니다.".into());
        }
        let before = link["before"].as_str();
        let same_kind = if ["cys", "wave"].contains(&name) { ["cys", "wave"] } else { ["cysd", "waved"] };
        if before.is_some_and(|v| !same_kind.iter().any(|n| Path::new(v) == base.join(n)))
            || !same_kind.iter().any(|n| Path::new(target) == base.join(n))
            || before.is_some_and(|v| v != target)
            || (before.is_none() && Path::new(target) != base.join(name)) {
            return Err("원래 명령 연결 기록이 허용된 경로와 일치하지 않습니다.".into());
        }
        let p = quote(path);
        let v = quote(target);
        if before.is_some() {
            script.push_str(&format!("[ -L {p} ] && [ \"$(/usr/bin/readlink {p})\" = {v} ] || exit 1\n"));
            continue;
        }
        let directory = Path::new(path).parent().unwrap().to_str().ok_or("명령 폴더 경로 오류")?;
        if action == "stage" {
            script.push_str(&format!("[ ! -e {p} ] && [ ! -L {p} ] || exit 1\n/bin/mkdir -p {d}\nq=$(/usr/bin/mktemp -d {template})\n/bin/chmod 700 \"$q\"\n/bin/ln -s {v} \"$q/entry\"\nr=$(/usr/bin/stat -f '%d:%i' \"$q/entry\")\nprintf '%s{{\"index\":{index},\"stage\":\"%s/entry\",\"inode\":\"%s\"}}' \"$comma\" \"$q\" \"$r\"\ncomma=','\n",
                d=quote(directory), template=quote(&format!("{directory}/.wave-stage-{attempt}.XXXXXXXX"))));
            continue;
        }
        let Some(stage) = link["stage"].as_str() else {
            if action == "cleanup" { continue; }
            if action == "rollback" { script.push_str(&format!("[ ! -e {p} ] && [ ! -L {p} ] || exit 1\n")); continue; }
            return Err("명령 연결 준비 영수증이 없습니다.".into());
        };
        let receipt = link["inode"].as_str().ok_or("명령 연결 생성 영수증 오류")?;
        let stage_path = Path::new(stage);
        let stage_dir = stage_path.parent().ok_or("준비 연결 경로 오류")?;
        if stage_path.file_name().and_then(|n| n.to_str()) != Some("entry")
            || stage_dir.parent() != Some(Path::new(directory))
            || !stage_dir.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&format!(".wave-stage-{attempt}.")))
            || receipt.is_empty() || !receipt.bytes().all(|b| b.is_ascii_digit() || b == b':') {
            return Err("명령 연결 준비 영수증 경로 또는 inode 오류".into());
        }
        let st = quote(stage);
        let r = quote(receipt);
        if action == "cleanup" {
            script.push_str(&format!("if [ -e {st} ] || [ -L {st} ]; then\n[ -L {st} ] && [ \"$(/usr/bin/readlink {st})\" = {v} ] && [ \"$(/usr/bin/stat -f '%d:%i' {st})\" = {r} ] || exit 1\n/bin/rm {st}\n/bin/rmdir {directory}\nfi\n", directory=quote(stage_dir.to_str().ok_or("준비 폴더 경로 오류")?)));
            continue;
        }
        let owns = format!("[ -L {p} ] && [ \"$(/usr/bin/readlink {p})\" = {v} ] && [ \"$(/usr/bin/stat -f '%d:%i' {p})\" = {r} ]");
        if action == "publish" {
            script.push_str(&format!("if {owns}; then :; else\n[ ! -e {p} ] && [ ! -L {p} ] || exit 1\n[ -L {st} ] && [ \"$(/usr/bin/readlink {st})\" = {v} ] && [ \"$(/usr/bin/stat -f '%d:%i' {st})\" = {r} ] || exit 1\n/bin/ln -P {st} {p}\n{owns} || exit 1\nfi\n"));
        } else if action == "rollback" {
            script.push_str(&format!("if [ -e {p} ] || [ -L {p} ]; then\n{owns} || exit 1\nq=$(/usr/bin/mktemp -d /private/tmp/wave-reclaim-XXXXXXXX)\n/bin/mv -nh {p} \"$q/entry\"\nif [ -e {p} ] || [ -L {p} ]; then echo \"연결이 바뀌어 $q 에 보존했습니다.\" >&2; exit 1; fi\nif [ -L \"$q/entry\" ] && [ \"$(/usr/bin/readlink \"$q/entry\")\" = {v} ] && [ \"$(/usr/bin/stat -f '%d:%i' \"$q/entry\")\" = {r} ]; then /bin/rm \"$q/entry\"; /bin/rmdir \"$q\"; else /bin/mv -nh \"$q/entry\" {p}; echo \"다른 연결을 보존했습니다: $q\" >&2; exit 1; fi\nfi\n"));
        } else { return Err("알 수 없는 명령 연결 단계".into()); }
    }
    if action == "stage" { script.push_str("printf ']'\n"); }
    Ok(script)
}

fn links(snapshot: &Value, action: &str) -> Result<String, String> {
    #[cfg(test)] {
        let _ = action;
        return TEST_SANDBOX.with(|sandbox| {
            if sandbox.borrow().is_none() { return Err("explicit validated sandbox root required".into()); }
            if !snapshot["links"].as_array().is_some_and(|links| links.is_empty()) {
                return Err("테스트에서는 실제 명령 연결 도구를 실행하지 않습니다.".into());
            }
            Ok("[]".into())
        });
    }
    #[cfg(not(test))] {
    let script = link_script(snapshot, action)?;
    let apple = format!("do shell script {} with administrator privileges", crate::applescript_str(&script));
    let output = Command::new("/usr/bin/osascript").args(["-e", &apple]).output()
        .map_err(|e| format!("명령 연결 권한 확인 실패: {e}"))?;
    if !output.status.success() { return Err(format!("명령 연결 작업이 중단됐습니다: {}", String::from_utf8_lossy(&output.stderr))); }
    String::from_utf8(output.stdout).map_err(|_| "명령 연결 영수증 응답 오류".into())
    }
}

pub(super) fn prepare(snapshot: &mut Value, persist: &mut dyn FnMut(&Value) -> Result<(), String>) -> Result<(), String> {
    if snapshot["new_job"]["kind"] != "absent" {
        return Err("기존 정식 자동 시작 작업을 변경하지 않습니다. 소유권 확인이 필요합니다.".into());
    }
    let (old, new) = paths();
    unchanged(&old, snapshot["old_xml"].as_str())?;
    unchanged(&new, snapshot["new_xml"].as_str())?;
    // No plist or loaded-job mutation before checkpoint: only reversible link repair.
    prepare_links(snapshot, persist)
}

fn prepare_links(snapshot: &mut Value, persist: &mut dyn FnMut(&Value) -> Result<(), String>) -> Result<(), String> {
    if snapshot["link_intents_persisted"] != true {
        let receipts: Vec<Value> = serde_json::from_str(links(snapshot, "stage")?.trim())
            .map_err(|e| format!("명령 연결 준비 영수증 해석 실패: {e}"))?;
        for receipt in receipts {
            let index = receipt["index"].as_u64().ok_or("명령 연결 영수증 번호 오류")? as usize;
            let link = snapshot["links"].as_array_mut().and_then(|a| a.get_mut(index)).ok_or("명령 연결 영수증 범위 오류")?;
            if !link["before"].is_null() { return Err("기존 연결의 준비 영수증을 허용하지 않습니다.".into()); }
            link["stage"] = receipt["stage"].clone();
            link["inode"] = receipt["inode"].clone();
        }
        snapshot["link_intents_persisted"] = json!(true);
    }
    persist(snapshot)?;
    links(snapshot, "publish")?;
    snapshot["links_published"] = json!(true);
    persist(snapshot)?;
    links(snapshot, "cleanup")?;
    Ok(())
}

pub(super) fn disable_legacy(snapshot: &mut Value, persist: &mut dyn FnMut(&Value) -> Result<(), String>) -> Result<(), String> {
    if snapshot["new_job"]["kind"] != "absent" {
        return Err("기존 정식 작업이 있어 등록 중지를 보류합니다.".into());
    }
    snapshot["legacy_disable_attempted"] = json!(true);
    persist(snapshot)?;
    let (old, _) = paths();
    unchanged(&old, snapshot["old_xml"].as_str())?;
    if snapshot["old_loaded"] == true {
        let current = old_job(snapshot)?;
        if current != snapshot["old_job"] { return Err("기존 작업이 캡처 후 바뀌어 중지하지 않습니다.".into()); }
        if current["kind"] == "running" {
            let owner: cys::rename_handover::ProcessIdentity = serde_json::from_value(snapshot["checkpoint_owner"].clone())
                .map_err(|_| "검증된 체크포인트 소유자 기록이 없습니다.")?;
            if current["identity"] != snapshot["checkpoint_owner"] || !cys::rename_handover::identity_alive(&owner)? {
                return Err("체크포인트 소유자와 적재된 기존 작업이 달라 중지하지 않습니다.".into());
            }
        } else if current["kind"] != "idle" {
            return Err("기존 작업의 유휴 상태를 재확인하지 못했습니다.".into());
        }
        let target = format!("gui/{}/{}", unsafe {libc::getuid()}, cys::launchd::LEGACY_LAUNCHD_LABEL);
        run("/bin/launchctl", &["bootout", &target])?;
    }
    if old_job(snapshot)?["kind"] != "absent" { return Err("기존 자동 시작 등록이 아직 실행 중입니다.".into()); }
    snapshot["legacy_disabled"] = json!(true);
    persist(snapshot)
}

pub(super) fn activate(snapshot: &mut Value, persist: &mut dyn FnMut(&Value) -> Result<(), String>) -> Result<(), String> {
    if snapshot["old_owner_stopped"] != true { return Err("기존 프로세스 종료 확인 기록이 없습니다.".into()); }
    let (old,new) = paths();
    if old_job(snapshot)?["kind"] != "absent" {
        return Err("기존 자동 시작 작업이 활성화돼 이중 시작을 보류합니다.".into());
    }
    let already_loaded = new_job(snapshot)?["kind"] != "absent";
    if already_loaded && (snapshot["new_publication"].is_null()
        || (snapshot["new_publication"]["existing"] == true && snapshot["loaded_new_job"].is_null())) {
        return Err("이 시도의 소유권 영수증이 없는 활성 작업을 변경하지 않습니다.".into());
    }
    let old_present = read_exact(&old)?;
    if old_present.is_some() || snapshot["old_removal_intent"] != true {
        unchanged(&old, snapshot["old_xml"].as_str())?;
    }
    stage_plist(snapshot, "new_publication", &new, "staged_xml", persist)?;
    publish_plist(&new, &snapshot["new_publication"], snapshot["staged_xml"].as_str().ok_or("준비 등록 오류")?)?;
    snapshot["activation_attempted"] = json!(true);
    snapshot["new_published"] = json!(true);
    persist(snapshot)?;
    cleanup_plist_stage(&snapshot["new_publication"], snapshot["staged_xml"].as_str().ok_or("준비 등록 오류")?)?;
    owns_file(&new, snapshot["new_publication"]["inode"].as_str().ok_or("등록 inode 영수증 오류")?)?;
    unchanged(&new, snapshot["staged_xml"].as_str())?;
    // Only this attempt's own load creates the load receipt rollback later relies on.
    if snapshot["loaded_new_job"].is_null() {
        if new_job(snapshot)?["kind"] != "absent" {
            return Err("이 시도가 적재하지 않은 정식 자동 시작 작업이 나타나 변경하지 않습니다. 복원 기록을 유지합니다.".into());
        }
        run("/bin/launchctl", &["load", "-w", new.to_str().ok_or("새 등록 경로 오류")?])?;
        owns_file(&new, snapshot["new_publication"]["inode"].as_str().ok_or("등록 inode 영수증 오류")?)?;
        let observed = new_job(snapshot)?;
        if observed["kind"] == "absent" { return Err("새 자동 시작 등록을 확인하지 못했습니다.".into()); }
        snapshot["loaded_new_job"] = observed;
        persist(snapshot)?;
    }
    if let Some(xml) = snapshot["old_xml"].as_str().map(str::to_string).filter(|_| old_present.is_some()) {
        snapshot["old_removal_intent"] = json!(true);
        persist(snapshot)?;
        remove_owned(&old, &xml, snapshot["old_inode"].as_str().ok_or("기존 등록 inode 영수증 오류")?)?;
        snapshot["old_removed"] = json!(true);
        persist(snapshot)?;
    }
    Ok(())
}

pub(super) fn rollback(snapshot: &mut Value, persist: &mut dyn FnMut(&Value) -> Result<(), String>) -> Result<(), String> {
    if snapshot["new_owner_stopped"] != true { return Err("새 프로세스 종료 확인 없이 등록을 복구하지 않습니다.".into()); }
    snapshot["rollback_intent"] = json!(true);
    persist(snapshot)?;
    let current_new = new_job(snapshot)?;
    if current_new["kind"] != "absent" {
        if snapshot["loaded_new_job"].is_null() {
            return Err("이 시도가 활성화하지 않은 정식 작업은 중지하지 않습니다.".into());
        }
        if snapshot["new_job"]["kind"] != "absent" || current_new["kind"] != "idle" {
            return Err("새 작업의 정지 증거 또는 이 시도 소유권이 없어 중지하지 않습니다.".into());
        }
        unchanged(&paths().1, snapshot["staged_xml"].as_str())?;
        owns_file(&paths().1, snapshot["new_publication"]["inode"].as_str().ok_or("이 시도의 등록 inode가 없습니다.")?)?;
        let target = format!("gui/{}/{}",unsafe {libc::getuid()},cys::launchd::LAUNCHD_LABEL);
        run("/bin/launchctl", &["bootout", &target])?;
    }
    let (old,new)=paths();
    let mut preserved_error = None;
    match snapshot["new_xml"].as_str() {
        Some(xml) => unchanged(&new, Some(xml))?,
        None => {
            match read_exact(&new) {
                Ok(None) => (),
                Ok(Some(xml)) => {
                    let owned = snapshot["new_publication"]["inode"].as_str()
                        .is_some_and(|inode| owns_file(&new, inode).is_ok());
                    if owned && Some(xml.as_str()) == snapshot["staged_xml"].as_str() {
                        remove_owned(&new, &xml, snapshot["new_publication"]["inode"].as_str().ok_or("등록 inode 오류")?)?;
                    } else {
                        preserved_error = Some("알 수 없는 새 등록 파일을 보존했습니다. 기존 등록을 복원했으며 업데이트는 완료하지 않았습니다.".to_string());
                    }
                }
                Err(error) => preserved_error = Some(format!("알 수 없는 새 등록을 보존했습니다: {error}")),
            }
            if let Some(error) = &preserved_error {
                snapshot["foreign_new_file_preserved"] = json!({"path":new,"error":error});
                persist(snapshot)?;
            }
        }
    }
    let current_old = old_job(snapshot)?;
    if current_old["kind"] != "absent" {
        if current_old != snapshot["old_job"] { return Err("기존 작업이 바뀌어 복원으로 덮어쓰지 않습니다.".into()); }
        unchanged(&old, snapshot["old_xml"].as_str())?;
    } else if let Some(xml) = snapshot["old_xml"].as_str().map(str::to_string) {
        let present = read_exact(&old)?;
        if present.as_deref() != Some(xml.as_str()) {
            if present.is_some() || snapshot["old_removal_intent"] != true { return Err("이 시도가 제거하지 않은 기존 등록은 덮어쓰지 않습니다.".into()); }
            stage_plist(snapshot, "old_recovery", &old, "old_xml", persist)?;
            publish_plist(&old, &snapshot["old_recovery"], &xml)?;
        }
    }
    if snapshot["old_loaded"] == true && current_old["kind"] == "absent" {
        if snapshot["old_owner_stopped"] != true || snapshot["legacy_disable_attempted"] != true {
            return Err("이 시도의 기존 작업 중지·종료 증거 없이 원래 작업을 다시 시작하지 않습니다.".into());
        }
        run("/bin/launchctl", &["load","-w",old.to_str().ok_or("복구 등록 경로 오류")?])?;
        if old_job(snapshot)?["kind"] == "absent" { return Err("기존 자동 시작 등록 복원을 확인하지 못했습니다.".into()); }
    }
    snapshot["old_registration_restored"] = json!(true);
    persist(snapshot)?;
    links(snapshot,"rollback")?;
    snapshot["links_rolled_back"] = json!(true);
    persist(snapshot)?;
    links(snapshot, "cleanup")?;
    for (key, text) in [("new_publication", "staged_xml"), ("old_recovery", "old_xml")] {
        if !snapshot[key].is_null() { cleanup_plist_stage(&snapshot[key], snapshot[text].as_str().ok_or("등록 원본 기록 오류")?)?; }
    }
    if let Some(error) = preserved_error { return Err(error); }
    snapshot["rollback_succeeded"] = json!(true);
    persist(snapshot)
}

/// One start request through the exact legacy registration this attempt's rollback restored.
pub(super) fn start_restored_owner(snapshot: &mut Value, persist: &mut dyn FnMut(&Value) -> Result<(), String>) -> Result<(), String> {
    if snapshot["old_registration_restored"] != true || snapshot["links_rolled_back"] != true || snapshot["old_loaded"] != true {
        return Err("복원 영수증에 이 시도가 되살린 기존 자동 시작 작업이 없어 서비스를 시작하지 않습니다. 복원 기록을 유지합니다.".into());
    }
    if snapshot["restored_owner_start_requested"] == true { return Ok(()); }
    if new_job(snapshot)?["kind"] != "absent" {
        return Err("정식 자동 시작 작업이 있어 기존 작업을 시작하지 않습니다. 두 서비스를 시작하지 않습니다.".into());
    }
    let (old, _) = paths();
    unchanged(&old, snapshot["old_xml"].as_str())?;
    let current = old_job(snapshot)?;
    snapshot["restored_owner_start_requested"] = json!(true);
    persist(snapshot)?;
    match current["kind"].as_str() {
        Some("absent") => run("/bin/launchctl", &["load", "-w", old.to_str().ok_or("복구 등록 경로 오류")?])?,
        // Non-forcing: never `-k`, so an owner that is already starting is not killed.
        Some("idle") => run("/bin/launchctl", &["kickstart",
            &format!("gui/{}/{}", unsafe {libc::getuid()}, cys::launchd::LEGACY_LAUNCHD_LABEL)])?,
        _ => {}
    }
    if old_job(snapshot)?["kind"] == "absent" {
        return Err("복원된 기존 자동 시작 작업을 확인하지 못했습니다. 복원 기록을 유지합니다.".into());
    }
    Ok(())
}

fn old_job(snapshot: &Value) -> Result<Value, String> {
    let base = Path::new(snapshot["daemon"].as_str().ok_or("실행 파일 기록 오류")?).parent().ok_or("실행 폴더 기록 오류")?;
    job(cys::launchd::LEGACY_LAUNCHD_LABEL, &paths().0,
        &[base.join("cysd"), base.join("waved"), "/opt/homebrew/bin/cysd".into(), "/usr/local/bin/cysd".into()])
}

fn new_job(snapshot: &Value) -> Result<Value, String> {
    job(cys::launchd::LAUNCHD_LABEL, &paths().1,
        &[PathBuf::from(snapshot["daemon"].as_str().ok_or("실행 파일 기록 오류")?)])
}

fn unchanged(path: &Path, expected: Option<&str>) -> Result<(), String> {
    if read_exact(path)?.as_deref() != expected { return Err("등록 파일이 캡처 후 바뀌어 기존 내용을 유지합니다.".into()); }
    Ok(())
}

fn inode_at(path: &Path) -> Result<Option<String>, String> {
    match path.symlink_metadata() {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(Some(format!("{}:{}", meta.dev(), meta.ino()))),
        Ok(_) => Err("등록 inode가 일반 파일이 아닙니다.".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("등록 inode 확인 실패: {e}")),
    }
}

fn owns_file(path: &Path, inode: &str) -> Result<(), String> {
    if inode_at(path)?.as_deref() != Some(inode) { return Err("이 시도의 등록 inode가 아닙니다. 다른 파일을 보존합니다.".into()); }
    Ok(())
}

fn private_directory(parent: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|e| format!("준비 폴더 식별자 생성 실패: {e}"))?;
    let nonce: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let directory = parent.join(format!(".wave-stage-{nonce}"));
    std::fs::DirBuilder::new().mode(0o700).create(&directory).map_err(|e| format!("전용 준비 폴더 생성 실패: {e}"))?;
    Ok(directory)
}

fn stage_plist(snapshot: &mut Value, key: &str, destination: &Path, text_key: &str,
    persist: &mut dyn FnMut(&Value) -> Result<(), String>) -> Result<(), String> {
    if !snapshot[key].is_null() { return persist(snapshot); }
    let desired = snapshot[text_key].as_str().ok_or("등록 원본 기록이 없습니다.")?.to_string();
    if read_exact(destination)?.as_deref() == Some(desired.as_str()) {
        let expected = if key == "new_publication" { snapshot["new_inode"].as_str() } else { snapshot["old_inode"].as_str() }
            .ok_or("기존 등록 inode 기록이 없습니다.")?;
        owns_file(destination, expected)?;
        snapshot[key] = json!({"inode":expected,"existing":true});
        return persist(snapshot);
    }
    unchanged(destination, None)?;
    let parent = destination.parent().ok_or("등록 폴더 경로 오류")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("등록 폴더 준비 실패: {e}"))?;
    let directory = private_directory(parent)?;
    let stage = directory.join("entry");
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&stage)
        .map_err(|e| format!("등록 파일 준비 실패: {e}"))?;
    file.write_all(desired.as_bytes()).and_then(|_| file.sync_all()).map_err(|e| format!("등록 준비 저장 실패: {e}"))?;
    let meta = file.metadata().map_err(|e| format!("등록 준비 inode 확인 실패: {e}"))?;
    std::fs::File::open(&directory).and_then(|f| f.sync_all()).and_then(|_| std::fs::File::open(parent)?.sync_all())
        .map_err(|e| format!("등록 준비 폴더 저장 실패: {e}"))?;
    snapshot[key] = json!({"stage":stage,"inode":format!("{}:{}",meta.dev(),meta.ino()),"existing":false});
    persist(snapshot)
}

fn publish_plist(destination: &Path, receipt: &Value, expected: &str) -> Result<(), String> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    let inode = receipt["inode"].as_str().ok_or("등록 준비 inode 영수증 오류")?;
    if inode_at(destination)?.as_deref() == Some(inode) { return unchanged(destination, Some(expected)); }
    unchanged(destination, None)?;
    let stage = Path::new(receipt["stage"].as_str().ok_or("등록 준비 파일 영수증 오류")?);
    owns_file(stage, inode)?;
    unchanged(stage, Some(expected))?;
    let directory = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(stage.parent().ok_or("등록 준비 폴더 오류")?).map_err(|e| format!("등록 준비 폴더 열기 실패: {e}"))?;
    let target = std::ffi::CString::new(destination.as_os_str().as_bytes()).map_err(|_| "등록 경로 NUL 오류")?;
    let entry = b"entry\0";
    // SAFETY: source directory handle pins the private staging directory; linkat never clobbers target.
    if unsafe { libc::linkat(directory.as_raw_fd(), entry.as_ptr().cast(), libc::AT_FDCWD, target.as_ptr(), 0) } != 0 {
        return Err(format!("등록 파일 독점 게시 실패: {}", std::io::Error::last_os_error()));
    }
    owns_file(destination, inode)?;
    unchanged(destination, Some(expected))?;
    std::fs::File::open(destination.parent().ok_or("등록 폴더 오류")?).and_then(|f| f.sync_all())
        .map_err(|e| format!("등록 게시 폴더 동기화 실패: {e}"))
}

fn remove_owned(path: &Path, expected: &str, inode: &str) -> Result<(), String> {
    owns_file(path, inode)?;
    unchanged(path, Some(expected))?;
    let directory = private_directory(path.parent().ok_or("등록 폴더 오류")?)?;
    let quarantine = directory.join("entry");
    rename_exclusive(path, &quarantine)?;
    if let Err(error) = owns_file(&quarantine, inode).and_then(|_| unchanged(&quarantine, Some(expected))) {
        // A raced-in foreign entry is preserved and restored only if its name is still free.
        if let Err(restore) = rename_exclusive(&quarantine, path) {
            return Err(format!("{error}; 원래 경로가 바뀌어 {}에 보존했습니다: {restore}", quarantine.display()));
        }
        return Err(error);
    }
    std::fs::remove_file(&quarantine).map_err(|e| format!("확인된 등록 파일 회수 실패: {e}"))?;
    let _ = std::fs::remove_dir(directory);
    std::fs::File::open(path.parent().ok_or("등록 폴더 경로 오류")?).and_then(|f| f.sync_all())
        .map_err(|e| format!("등록 폴더 동기화 실패: {e}"))
}

fn cleanup_plist_stage(receipt: &Value, expected: &str) -> Result<(), String> {
    let Some(stage) = receipt["stage"].as_str() else { return Ok(()); };
    let path = Path::new(stage);
    if inode_at(path)?.is_none() { return Ok(()); }
    remove_owned(path, expected, receipt["inode"].as_str().ok_or("등록 준비 inode 영수증 오류")?)?;
    std::fs::remove_dir(path.parent().ok_or("등록 준비 폴더 오류")?).map_err(|e| format!("등록 준비 폴더 정리 실패: {e}"))
}

fn rename_exclusive(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    let source = std::ffi::CString::new(source.as_os_str().as_bytes()).map_err(|_| "등록 경로에 NUL 문자가 있습니다.")?;
    let target = std::ffi::CString::new(target.as_os_str().as_bytes()).map_err(|_| "등록 경로에 NUL 문자가 있습니다.")?;
    // SAFETY: owned NUL-terminated strings remain live; RENAME_EXCL never replaces a destination.
    if unsafe { libc::renameatx_np(libc::AT_FDCWD, source.as_ptr(), libc::AT_FDCWD, target.as_ptr(), libc::RENAME_EXCL) } != 0 {
        return Err(format!("등록 파일 안전 이동 실패: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(test)]
struct Sandbox {
    root: PathBuf,
    jobs: std::collections::HashMap<String, Value>,
    commands: Vec<Vec<String>>,
    owners: usize,
    peak_owners: usize,
    active_handover: bool,
    /// Test seam: another actor loads the canonical label right after this attempt publishes its plist.
    foreign_load_after_publish: bool,
}

#[cfg(test)]
thread_local! {
    static TEST_SANDBOX: std::cell::RefCell<Option<Sandbox>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn sandbox_command(program: &str, args: &[&str]) -> Result<(), String> {
    TEST_SANDBOX.with(|sandbox| {
        let mut sandbox = sandbox.borrow_mut();
        let sandbox = sandbox.as_mut().ok_or("explicit validated sandbox root required")?;
        if program != "/bin/launchctl" { return Err("테스트의 실제 명령 실행을 금지합니다.".into()); }
        sandbox.commands.push(args.iter().map(|s| s.to_string()).collect());
        match args {
            ["bootout", target] if target.ends_with(cys::launchd::LEGACY_LAUNCHD_LABEL) => {
                sandbox.jobs.remove(cys::launchd::LEGACY_LAUNCHD_LABEL);
                sandbox.owners = 0;
            }
            ["load", "-w", path] if Path::new(path) == sandbox.root.join("LaunchAgents/old.plist") => {
                sandbox.jobs.insert(cys::launchd::LEGACY_LAUNCHD_LABEL.into(), json!({"kind":"idle","definition":"sandbox-old"}));
                sandbox.owners += 1;
                sandbox.peak_owners = sandbox.peak_owners.max(sandbox.owners);
            }
            ["kickstart", target] if target.ends_with(&format!("/{}", cys::launchd::LEGACY_LAUNCHD_LABEL)) => {
                sandbox.jobs.insert(cys::launchd::LEGACY_LAUNCHD_LABEL.into(), json!({"kind":"running","definition":"sandbox-old"}));
                sandbox.owners += 1;
                sandbox.peak_owners = sandbox.peak_owners.max(sandbox.owners);
            }
            ["load", "-w", path] if Path::new(path) == sandbox.root.join("LaunchAgents/new.plist") => {
                sandbox.jobs.insert(cys::launchd::LAUNCHD_LABEL.into(), json!({"kind":"idle","definition":"sandbox-new"}));
                sandbox.owners += 1;
                sandbox.peak_owners = sandbox.peak_owners.max(sandbox.owners);
            }
            ["bootout", target] if target.ends_with(&format!("/{}", cys::launchd::LAUNCHD_LABEL)) => {
                sandbox.jobs.remove(cys::launchd::LAUNCHD_LABEL);
                sandbox.owners = sandbox.owners.saturating_sub(1);
            }
            _ => return Err("이 샌드박스에서는 새 데몬 또는 실제 등록을 실행하지 않습니다.".into()),
        }
        Ok(())
    })
}

#[cfg(test)]
fn validated_sandbox_root(raw: Option<std::ffi::OsString>) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw.ok_or("WAVE_TEST_ROOT가 필요합니다.")?);
    let families = ["/private/tmp", "/tmp", "/private/var/folders", "/var/folders"];
    if !path.is_absolute() || !families.iter().any(|p| path.starts_with(p) && path != Path::new(p))
        || path.components().any(|c| ["Applications", "LaunchAgents", ".cys"].iter().any(|p| c.as_os_str() == std::ffi::OsStr::new(p))) {
        return Err("WAVE_TEST_ROOT는 시스템 경로가 아닌 전용 임시 폴더여야 합니다.".into());
    }
    let metadata = path.symlink_metadata().map_err(|e| format!("WAVE_TEST_ROOT 확인 실패: {e}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata.mode() & 0o777 != 0o700
        || metadata.uid() != unsafe { libc::geteuid() } {
        return Err("WAVE_TEST_ROOT는 현재 사용자 소유의 비공개 일반 폴더여야 합니다.".into());
    }
    let canonical = path.canonicalize().map_err(|e| format!("WAVE_TEST_ROOT 해소 실패: {e}"))?;
    if !families.iter().any(|p| canonical.starts_with(p) && canonical != Path::new(p)) {
        return Err("WAVE_TEST_ROOT가 전용 임시 경계를 벗어났습니다.".into());
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct LinkFixture(PathBuf);
    impl LinkFixture {
        fn new() -> Self {
            let root = validated_sandbox_root(std::env::var_os("WAVE_TEST_ROOT")).expect("reviewed explicit private root required");
            let fixture = private_directory(&root).unwrap();
            TEST_SANDBOX.with(|sandbox| *sandbox.borrow_mut() = Some(Sandbox {
                root:fixture.clone(), jobs:std::collections::HashMap::new(), commands:vec![],
                owners:0, peak_owners:0, active_handover:false, foreign_load_after_publish:false,
            }));
            for directory in bin_directories().unwrap() { std::fs::create_dir(directory).unwrap(); }
            Self(fixture)
        }
    }
    impl Drop for LinkFixture {
        fn drop(&mut self) {
            TEST_SANDBOX.with(|s| *s.borrow_mut() = None);
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn link_plan_rejects_foreign_directories_and_targets_without_running_commands() {
        let fixture = LinkFixture::new();
        let base = fixture.0.join("bundle");
        let allowed = bin_directories().unwrap()[0].join("wave");
        let mut snapshot = json!({"daemon":base.join("waved"), "links":[{
            "path":fixture.0.join("outside/wave"), "before":null, "after":base.join("wave")
        }]});
        snapshot["attempt_id"] = json!("0123456789abcdef0123456789abcdef");
        assert!(link_script(&snapshot, "stage").is_err());
        snapshot["links"][0]["path"] = json!(allowed);
        snapshot["links"][0]["after"] = json!(fixture.0.join("foreign/wave"));
        assert!(link_script(&snapshot, "stage").is_err());
        snapshot["links"][0]["after"] = json!(base.join("wave"));
        snapshot["links"][0]["before"] = json!(fixture.0.join("foreign/wave"));
        assert!(link_script(&snapshot, "rollback").is_err());
        snapshot["links"][0]["before"] = Value::Null;
        assert!(link_script(&snapshot, "stage").is_ok());
    }

    #[test]
    fn cli_link_capture_preserves_same_kind_and_plans_only_missing_names() {
        let fixture = LinkFixture::new();
        let base = fixture.0.join("bundle");
        let existing = bin_directories().unwrap()[0].join("cys");
        std::os::unix::fs::symlink(base.join("wave"), &existing).unwrap();
        let before = existing.symlink_metadata().unwrap().ino();
        let links = capture_links(&base).unwrap();
        assert_eq!(links.len(), 8);
        let kept = links.iter().find(|link| link["path"].as_str() == existing.to_str()).unwrap();
        assert_eq!(kept["before"], kept["after"]);
        assert_eq!(links.iter().filter(|link| link["before"].is_null()).count(), 7);
        assert_eq!(existing.symlink_metadata().unwrap().ino(), before);
        assert_eq!(std::fs::read_link(existing).unwrap(), base.join("wave"));
        let snapshot = json!({"daemon":base.join("waved"), "attempt_id":"0123456789abcdef0123456789abcdef", "links":links});
        assert!(link_script(&snapshot, "stage").is_ok());
        TEST_SANDBOX.with(|sandbox| assert!(sandbox.borrow().as_ref().unwrap().commands.is_empty()));
    }

    #[test]
    fn standalone_cli_link_repair_refuses_handover_and_preserves_foreign_file() {
        let fixture = LinkFixture::new();
        let base = fixture.0.join("bundle");
        let file = bin_directories().unwrap()[0].join("wave");
        std::fs::write(&file, "user command").unwrap();
        let inode = inode_at(&file).unwrap();
        TEST_SANDBOX.with(|sandbox| sandbox.borrow_mut().as_mut().unwrap().active_handover = true);
        assert!(repair_cli_links(Path::new("relative"), Path::new("relative")).unwrap_err().contains("업데이트"));
        TEST_SANDBOX.with(|sandbox| sandbox.borrow_mut().as_mut().unwrap().active_handover = false);
        assert!(repair_cli_links(&base.join("wave"), &base.join("waved")).unwrap_err().contains("일반 파일"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "user command");
        assert_eq!(inode_at(&file).unwrap(), inode);
        TEST_SANDBOX.with(|sandbox| {
            let sandbox = sandbox.borrow();
            let sandbox = sandbox.as_ref().unwrap();
            assert!(sandbox.commands.is_empty());
            assert!(sandbox.jobs.is_empty());
        });
    }

    #[test]
    fn standalone_cli_link_repair_is_noop_when_all_managed_aliases_exist() {
        let fixture = LinkFixture::new();
        let base = fixture.0.join("bundle");
        let mut identities = Vec::new();
        for directory in bin_directories().unwrap() {
            for name in ["cys", "cysd", "wave", "waved"] {
                let path = directory.join(name);
                std::os::unix::fs::symlink(base.join(name), &path).unwrap();
                identities.push((path.clone(), path.symlink_metadata().unwrap().ino()));
            }
        }
        repair_cli_links(&base.join("wave"), &base.join("waved")).unwrap();
        for (path, inode) in identities { assert_eq!(path.symlink_metadata().unwrap().ino(), inode); }
        TEST_SANDBOX.with(|sandbox| {
            let sandbox = sandbox.borrow();
            let sandbox = sandbox.as_ref().unwrap();
            assert!(sandbox.commands.is_empty());
            assert!(sandbox.jobs.is_empty());
        });
    }

    #[test]
    fn privileged_link_scripts_never_force_clobber_or_remove_unverified_entries() {
        // Maintenance contract (rename-r1 22:22): detect + fail closed; never force-replace or delete
        // an entry whose kind/target/inode was not re-verified in the same privileged shell.
        let fixture = LinkFixture::new();
        let base = fixture.0.join("bundle");
        let directory = bin_directories().unwrap()[0].clone();
        let attempt = "0123456789abcdef0123456789abcdef";
        let stage = directory.join(format!(".wave-stage-{attempt}.ABCDEFGH/entry"));
        let snapshot = json!({"daemon":base.join("waved"), "attempt_id":attempt, "links":[{
            "path":directory.join("wave"), "before":null, "after":base.join("wave"),
            "stage":stage, "inode":"1:2"}]});
        for action in ["publish", "rollback", "cleanup"] {
            let script = link_script(&snapshot, action).unwrap();
            for forbidden in ["mv -f", "mv -fh", "ln -sf", "ln -f", "rm -f", "rm -r"] {
                assert!(!script.contains(forbidden), "{action} uses {forbidden}");
            }
            for (index, line) in script.lines().enumerate() {
                if line.contains("/bin/rm ") {
                    let guarded = script.lines().take(index + 1).collect::<Vec<_>>().join("\n");
                    assert!(guarded.contains("readlink") && guarded.contains("%d:%i"), "{action}: unverified rm");
                }
            }
        }
        assert!(link_script(&snapshot, "rollback").unwrap().contains("/bin/mv -nh"));
        TEST_SANDBOX.with(|sandbox| assert!(sandbox.borrow().as_ref().unwrap().commands.is_empty()));
    }

    #[test]
    fn injected_root_refuses_unset_and_system_locations_before_io() {
        assert!(validated_sandbox_root(None).is_err());
        for path in ["", ".", "/Applications", "/usr/local/bin", "/opt/homebrew/bin", "/Users/user/.cys"] {
            assert!(validated_sandbox_root(Some(path.into())).is_err());
        }
    }

    #[test]
    fn tampered_new_plist_is_preserved_while_captured_old_registration_recovers() {
        let root = validated_sandbox_root(std::env::var_os("WAVE_TEST_ROOT")).expect("reviewed explicit private root required");
        let fixture = private_directory(&root).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                TEST_SANDBOX.with(|s| *s.borrow_mut() = None);
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let _cleanup = Cleanup(fixture.clone());
        std::fs::create_dir(fixture.join("LaunchAgents")).unwrap();
        TEST_SANDBOX.with(|sandbox| *sandbox.borrow_mut() = Some(Sandbox {
            root:fixture.clone(), jobs:std::collections::HashMap::from([
                (cys::launchd::LEGACY_LAUNCHD_LABEL.into(), json!({"kind":"idle","definition":"sandbox-old"}))]),
            commands:vec![], owners:0, peak_owners:0, active_handover:false, foreign_load_after_publish:false,
        }));
        let (old, new) = paths();
        std::fs::write(&old, "captured-old-registration").unwrap();
        let mut snapshot = json!({"attempt_id":"0123456789abcdef0123456789abcdef",
            "daemon":fixture.join("waved"), "old_xml":"captured-old-registration", "new_xml":null,
            "old_inode":inode_at(&old).unwrap(), "new_inode":null, "staged_xml":"trusted-new-registration",
            "old_job":{"kind":"idle","definition":"sandbox-old"}, "new_job":{"kind":"absent"},
            "old_loaded":true, "new_loaded":false, "links":[], "old_owner_stopped":true, "new_owner_stopped":true});
        let marker = fixture.join("receipt.json");
        let mut persist = |value: &Value| std::fs::write(&marker, serde_json::to_vec(value).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string());
        disable_legacy(&mut snapshot, &mut persist).unwrap();
        assert_eq!(old_job(&snapshot).unwrap()["kind"], "absent");
        // Tamper after old registration disable, before any canonical activation.
        std::fs::write(&new, "FOREIGN: keep these bytes").unwrap();
        let foreign_inode = inode_at(&new).unwrap();
        let activation_error = activate(&mut snapshot, &mut persist).unwrap_err();
        assert!(activation_error.contains("등록"));
        let rollback_error = rollback(&mut snapshot, &mut persist).unwrap_err();
        assert!(rollback_error.contains("보존"));
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "FOREIGN: keep these bytes");
        assert_eq!(inode_at(&new).unwrap(), foreign_inode);
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "captured-old-registration");
        assert_ne!(old_job(&snapshot).unwrap()["kind"], "absent");
        assert_eq!(new_job(&snapshot).unwrap()["kind"], "absent");
        let kept: Value = serde_json::from_slice(&std::fs::read(marker).unwrap()).unwrap();
        assert_eq!(kept["old_registration_restored"], true);
        assert!(kept["foreign_new_file_preserved"].is_object());
        assert_ne!(kept["rollback_succeeded"], true);
        TEST_SANDBOX.with(|sandbox| {
            let sandbox = sandbox.borrow();
            let sandbox = sandbox.as_ref().unwrap();
            assert_eq!(sandbox.peak_owners, 1);
            assert_eq!(sandbox.commands.len(), 2); // only old bootout, then captured old load
            assert!(sandbox.commands.iter().all(|args| !args.iter().any(|arg| arg == new.to_str().unwrap())));
        });
    }

    /// Rooted sandbox with a captured, loaded legacy job; the legacy job is disabled by this attempt.
    struct Attempt { fixture: PathBuf, marker: PathBuf, snapshot: Value }
    impl Drop for Attempt {
        fn drop(&mut self) {
            TEST_SANDBOX.with(|s| *s.borrow_mut() = None);
            std::fs::remove_dir_all(&self.fixture).unwrap();
        }
    }
    impl Attempt {
        fn disabled_legacy(foreign_load_after_publish: bool) -> Self {
            let root = validated_sandbox_root(std::env::var_os("WAVE_TEST_ROOT")).expect("reviewed explicit private root required");
            let fixture = private_directory(&root).unwrap();
            std::fs::create_dir(fixture.join("LaunchAgents")).unwrap();
            TEST_SANDBOX.with(|sandbox| *sandbox.borrow_mut() = Some(Sandbox {
                root:fixture.clone(), jobs:std::collections::HashMap::from([
                    (cys::launchd::LEGACY_LAUNCHD_LABEL.into(), json!({"kind":"idle","definition":"sandbox-old"}))]),
                commands:vec![], owners:0, peak_owners:0, active_handover:false, foreign_load_after_publish,
            }));
            let (old, _) = paths();
            std::fs::write(&old, "captured-old-registration").unwrap();
            let snapshot = json!({"attempt_id":"0123456789abcdef0123456789abcdef",
                "daemon":fixture.join("waved"), "old_xml":"captured-old-registration", "new_xml":null,
                "old_inode":inode_at(&old).unwrap(), "new_inode":null, "staged_xml":"trusted-new-registration",
                "old_job":{"kind":"idle","definition":"sandbox-old"}, "new_job":{"kind":"absent"},
                "old_loaded":true, "new_loaded":false, "links":[], "old_owner_stopped":true});
            let mut attempt = Attempt { marker: fixture.join("receipt.json"), fixture, snapshot };
            let mut snapshot = attempt.snapshot.take();
            disable_legacy(&mut snapshot, &mut attempt.persist()).unwrap();
            attempt.snapshot = snapshot;
            attempt
        }
        fn persist(&self) -> impl FnMut(&Value) -> Result<(), String> + '_ {
            move |value: &Value| std::fs::write(&self.marker, serde_json::to_vec(value).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        }
        fn step(&mut self, action: fn(&mut Value, &mut dyn FnMut(&Value) -> Result<(), String>) -> Result<(), String>) -> Result<(), String> {
            let mut snapshot = self.snapshot.take();
            let result = action(&mut snapshot, &mut self.persist());
            self.snapshot = snapshot;
            result
        }
        fn kept(&self) -> Value { serde_json::from_slice(&std::fs::read(&self.marker).unwrap()).unwrap() }
        fn commands(&self) -> Vec<Vec<String>> {
            TEST_SANDBOX.with(|s| s.borrow().as_ref().unwrap().commands.clone())
        }
    }

    fn canonical_bootouts(commands: &[Vec<String>]) -> usize {
        commands.iter().filter(|args| args[0] == "bootout"
            && args[1].ends_with(&format!("/{}", cys::launchd::LAUNCHD_LABEL))).count()
    }

    #[test]
    fn rollback_keeps_foreign_canonical_job_when_activation_was_refused() {
        // Reviewer scenario: activation attempted, a foreign idle job with the known staged bytes appears,
        // activate refuses at its first ownership check; rollback must not touch that job or file.
        let mut attempt = Attempt::disabled_legacy(false);
        let (_, new) = paths();
        std::fs::write(&new, "trusted-new-registration").unwrap();
        let foreign_inode = inode_at(&new).unwrap();
        TEST_SANDBOX.with(|s| { s.borrow_mut().as_mut().unwrap().jobs.insert(
            cys::launchd::LAUNCHD_LABEL.into(), json!({"kind":"idle","definition":"foreign-canonical"})); });
        attempt.snapshot["activation_attempted"] = json!(true);
        assert!(attempt.step(activate).is_err());
        attempt.snapshot["new_owner_stopped"] = json!(true);
        let error = attempt.step(rollback).unwrap_err();
        assert!(error.contains("중지하지 않습니다"), "{error}");
        assert_eq!(new_job(&attempt.snapshot).unwrap()["definition"], "foreign-canonical");
        assert_eq!(inode_at(&new).unwrap(), foreign_inode);
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "trusted-new-registration");
        assert_eq!(canonical_bootouts(&attempt.commands()), 0);
        assert_ne!(attempt.kept()["rollback_succeeded"], true);
    }

    #[test]
    fn foreign_load_after_publication_is_never_recorded_as_this_attempts_load() {
        let mut attempt = Attempt::disabled_legacy(true);
        let (_, new) = paths();
        let activation = attempt.step(activate);
        assert!(activation.is_err(), "a job this attempt did not load must not be adopted");
        assert!(attempt.kept()["loaded_new_job"].is_null(), "no load receipt for a foreign load");
        attempt.snapshot["new_owner_stopped"] = json!(true);
        assert!(attempt.step(rollback).is_err());
        assert_eq!(new_job(&attempt.snapshot).unwrap()["definition"], "foreign-canonical");
        assert!(new.exists(), "the plist a loaded foreign job uses is kept");
        assert_eq!(canonical_bootouts(&attempt.commands()), 0);
        assert!(!attempt.commands().iter().any(|args| args[0] == "load" && args[2] == new.to_str().unwrap()));
    }

    #[test]
    fn rollback_unloads_and_removes_only_the_job_this_attempt_loaded() {
        // Positive control: with this attempt's own load receipt, rollback restores the captured old job.
        let mut attempt = Attempt::disabled_legacy(false);
        let (old, new) = paths();
        attempt.step(activate).unwrap();
        assert_eq!(attempt.kept()["loaded_new_job"]["definition"], "sandbox-new");
        assert!(!old.exists(), "activation removed the old plist this attempt owned");
        attempt.snapshot["new_owner_stopped"] = json!(true);
        attempt.step(rollback).unwrap();
        assert_eq!(new_job(&attempt.snapshot).unwrap()["kind"], "absent");
        assert!(!new.exists());
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "captured-old-registration");
        assert_ne!(old_job(&attempt.snapshot).unwrap()["kind"], "absent");
        assert_eq!(canonical_bootouts(&attempt.commands()), 1);
        TEST_SANDBOX.with(|s| assert_eq!(s.borrow().as_ref().unwrap().peak_owners, 1));
        assert_eq!(attempt.kept()["rollback_succeeded"], true);
    }

    fn restored_receipt(attempt: &mut Attempt) {
        for key in ["old_registration_restored", "links_rolled_back", "new_owner_stopped"] { attempt.snapshot[key] = json!(true); }
    }

    #[test]
    fn restored_owner_start_loads_only_the_receipted_legacy_job_once() {
        let mut attempt = Attempt::disabled_legacy(false);
        let (old, _) = paths();
        restored_receipt(&mut attempt);
        let before = attempt.commands().len();
        attempt.step(start_restored_owner).unwrap();
        let commands = attempt.commands();
        assert_eq!(commands[before..], [vec!["load".to_string(), "-w".into(), old.to_str().unwrap().into()]]);
        assert_eq!(attempt.kept()["restored_owner_start_requested"], true);
        attempt.step(start_restored_owner).unwrap();
        assert_eq!(attempt.commands().len(), before + 1, "ONE start request only");
    }

    #[test]
    fn restored_owner_start_kickstarts_an_idle_legacy_job_without_force() {
        let mut attempt = Attempt::disabled_legacy(false);
        restored_receipt(&mut attempt);
        TEST_SANDBOX.with(|s| { s.borrow_mut().as_mut().unwrap().jobs.insert(
            cys::launchd::LEGACY_LAUNCHD_LABEL.into(), json!({"kind":"idle","definition":"sandbox-old"})); });
        let before = attempt.commands().len();
        attempt.step(start_restored_owner).unwrap();
        let last = &attempt.commands()[before];
        assert_eq!(last[0], "kickstart");
        assert!(!last.iter().any(|arg| arg == "-k"), "never force-restart");
        assert!(last[1].ends_with(cys::launchd::LEGACY_LAUNCHD_LABEL));
    }

    #[test]
    fn restored_owner_start_refuses_without_receipt_or_on_mismatch() {
        // No receipt.
        let mut attempt = Attempt::disabled_legacy(false);
        let before = attempt.commands().len();
        assert!(attempt.step(start_restored_owner).is_err());
        // Receipt, but the legacy definition changed after restoration.
        restored_receipt(&mut attempt);
        std::fs::write(paths().0, "changed-by-someone").unwrap();
        assert!(attempt.step(start_restored_owner).is_err());
        std::fs::write(paths().0, "captured-old-registration").unwrap();
        // Receipt, but a canonical job is loaded: never two owners.
        TEST_SANDBOX.with(|s| { s.borrow_mut().as_mut().unwrap().jobs.insert(
            cys::launchd::LAUNCHD_LABEL.into(), json!({"kind":"idle","definition":"foreign-canonical"})); });
        assert!(attempt.step(start_restored_owner).is_err());
        // Original was not a loaded registration (CLI-autostart original): never pretend one exists.
        TEST_SANDBOX.with(|s| { s.borrow_mut().as_mut().unwrap().jobs.remove(cys::launchd::LAUNCHD_LABEL); });
        attempt.snapshot["old_loaded"] = json!(false);
        assert!(attempt.step(start_restored_owner).is_err());
        assert_eq!(attempt.commands().len(), before, "no launchctl call on any refusal");
        assert_ne!(attempt.snapshot["restored_owner_start_requested"], true);
    }
}
