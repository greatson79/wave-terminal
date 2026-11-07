//! Shared autostart barrier for the executable-name handover; state paths stay unchanged.
use std::path::{Path, PathBuf};

pub fn marker_path() -> PathBuf {
    crate::home_dir().join(".cys/.pending-restore")
}

pub fn autostart_blocked(socket: &Path) -> Result<bool, String> {
    autostart_blocked_at(&marker_path(), socket)
}

pub fn read_marker() -> Result<Option<serde_json::Value>, String> {
    read_marker_at(&marker_path())
}

fn read_marker_at(marker: &Path) -> Result<Option<serde_json::Value>, String> {
    match std::fs::symlink_metadata(marker) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() =>
            return Err("업데이트 복원 표지가 일반 파일이 아닙니다. 기존 설정을 유지합니다.".into()),
        Err(e) => return Err(format!("업데이트 복원 표지를 확인하지 못했습니다: {e}")),
        _ => {}
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let mut file = options.open(marker)
        .map_err(|e| format!("업데이트 복원 표지를 안전하게 열지 못했습니다: {e}"))?;
    let meta = file.metadata().map_err(|e| format!("업데이트 복원 표지 확인 실패: {e}"))?;
    if !meta.is_file() { return Err("업데이트 복원 표지가 일반 파일이 아닙니다.".into()); }
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 { return Err("업데이트 복원 표지가 재분석 링크입니다.".into()); }
    }
    let mut text = String::new();
    std::io::Read::read_to_string(&mut file, &mut text)
        .map_err(|e| format!("업데이트 복원 표지를 읽지 못했습니다: {e}"))?;
    if text.trim().is_empty() { return Ok(None); }
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("업데이트 복원 표지가 손상됐습니다. 자동 시작을 보류합니다: {e}"))?;
    let record = value.get("rename_handover")
        .ok_or("업데이트 이름 변경 기록을 확인할 수 없습니다. 자동 시작을 보류합니다.")?;
    let phase = record.get("phase").and_then(|v| v.as_str()).unwrap_or("");
    if !matches!(phase, "prepared" | "stopping-old" | "starting-new" | "restoring" | "failed" | "awaiting-layout")
        || record.get("socket").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).is_none()
    {
        return Err("업데이트 이름 변경 단계가 올바르지 않습니다. 자동 시작을 보류합니다.".into());
    }
    Ok(Some(value))
}

fn autostart_blocked_at(marker: &Path, socket: &Path) -> Result<bool, String> {
    Ok(read_marker_at(marker)?.is_some_and(|v| {
        v["rename_handover"]["phase"] != "awaiting-layout"
            && v["rename_handover"]["socket"].as_str() == socket.to_str()
    }))
}

/// Called only by the existing single-instance GUI coordinator, the sole marker writer.
/// Publish while the old owner (checked on both sides) or its startup guard excludes a new owner.
pub fn prepare_marker(socket: &Path, value: &serde_json::Value, old: Option<&ProcessIdentity>) -> Result<(), String> {
    let record = &value["rename_handover"];
    if record["phase"] != "prepared" || record["socket"].as_str() != socket.to_str()
        || record.get("old_identity") != Some(&serde_json::to_value(old).map_err(|e| e.to_string())?) {
        return Err("업데이트 시작 기록이 확인한 소켓과 프로세스에 결속되지 않았습니다.".into());
    }
    if read_marker()?.is_some() {
        return Err("이전 업데이트 이관 기록이 남아 있습니다. 복구 안내를 확인하세요.".into());
    }
    if let Some(old) = old {
        if !identity_alive(old)? {
            return Err("이전 프로세스가 바뀌었습니다. 업데이트를 다시 시도하세요.".into());
        }
        write_marker(Some(value))?;
        // Success requires the same old owner to hold the startup guard across publication.
        if !identity_alive(old)? {
            return Err("업데이트 시작 기록을 쓰는 동안 소유권이 바뀌었습니다. 자동 시작을 보류합니다.".into());
        }
        return Ok(());
    }
    #[cfg(unix)] {
        let lock = acquire_startup_guard(socket)?;
        if read_marker()?.is_some() {
            return Err("다른 업데이트 이관이 이미 시작됐습니다. 기존 기록을 유지합니다.".into());
        }
        let result = write_marker(Some(value));
        drop(lock);
        result
    }
    #[cfg(windows)] {
        let guard = tokio::net::windows::named_pipe::ServerOptions::new()
            .first_pipe_instance(true).create(socket)
            .map_err(|e| format!("업데이트 시작 소유권을 확보하지 못했습니다: {e}"))?;
        if read_marker()?.is_some() {
            return Err("다른 업데이트 이관이 이미 시작됐습니다. 기존 기록을 유지합니다.".into());
        }
        let result = write_marker(Some(value));
        drop(guard);
        result
    }
}

#[cfg(unix)]
fn acquire_startup_guard(socket: &Path) -> Result<std::fs::File, String> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    let parent = socket.parent().ok_or("업데이트 소켓 경로가 올바르지 않습니다.")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("업데이트 소켓 폴더 생성 실패: {e}"))?;
    let file = std::fs::OpenOptions::new().write(true).create(true).truncate(false)
        .custom_flags(libc::O_NOFOLLOW).mode(0o600).open(socket.with_extension("lock"))
        .map_err(|e| format!("업데이트 시작 소유권 확인 실패: {e}"))?;
    if unsafe {libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB)} != 0 {
        return Err("다른 프로세스가 시작 중입니다. 업데이트를 잠시 후 다시 시도하세요.".into());
    }
    Ok(file)
}

/// Atomically update the existing restore marker; never follow a user-supplied symlink.
pub fn write_marker(value: Option<&serde_json::Value>) -> Result<(), String> {
    write_marker_at(&marker_path(), value)
}

fn write_marker_at(marker: &Path, value: Option<&serde_json::Value>) -> Result<(), String> {
    if marker.exists() || marker.symlink_metadata().is_ok() { read_marker_at(&marker)?; }
    let parent = marker.parent().ok_or("업데이트 복원 표지 경로가 올바르지 않습니다.")?;
    let parent_existed = parent.try_exists().map_err(|e| format!("업데이트 복원 폴더 확인 실패: {e}"))?;
    std::fs::create_dir_all(parent).map_err(|e| format!("업데이트 복원 폴더 생성 실패: {e}"))?;
    #[cfg(unix)] if !parent_existed {
        let ancestor = parent.parent().ok_or("업데이트 복원 상위 폴더가 없습니다.")?;
        std::fs::File::open(ancestor).and_then(|f| f.sync_all())
            .map_err(|e| format!("업데이트 복원 상위 폴더 동기화 실패: {e}"))?;
    }
    #[cfg(not(unix))] let _ = parent_existed;
    let temp = parent.join(format!(".pending-restore.{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(|e| format!("업데이트 복원 표지 준비 실패: {e}"))?;
    let mut renamed = false;
    let result = (|| -> Result<(), String> {
        use std::io::Write;
        let bytes = value.map(serde_json::to_vec).transpose()
            .map_err(|e| format!("업데이트 복원 기록 변환 실패: {e}"))?.unwrap_or_default();
        file.write_all(&bytes).and_then(|_| file.sync_all())
            .map_err(|e| format!("업데이트 복원 기록 저장 실패: {e}"))?;
        drop(file);
        durable_replace(&temp, marker)?;
        renamed = true;
        #[cfg(unix)] std::fs::File::open(parent).and_then(|f| f.sync_all())
            .map_err(|e| format!("업데이트 복원 폴더 동기화 실패: {e}"))?;
        Ok(())
    })();
    if result.is_err() && !renamed { let _ = std::fs::remove_file(&temp); }
    result
}

#[cfg(not(windows))]
fn durable_replace(source: &Path, target: &Path) -> Result<(), String> {
    std::fs::rename(source, target).map_err(|e| format!("업데이트 복원 표지 교체 실패: {e}"))
}

#[cfg(windows)]
fn durable_replace(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe {MoveFileExW(source.as_ptr(), target.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)} == 0 {
        Err(format!("업데이트 복원 표지 교체 실패: {}", std::io::Error::last_os_error()))
    } else { Ok(()) }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time: u64,
    pub executable: PathBuf,
}

pub fn process_identity(pid: u32) -> Result<Option<ProcessIdentity>, String> {
    let pid = sysinfo::Pid::from_u32(pid);
    let mut system = sysinfo::System::new();
    system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::Some(&[pid]), true,
        sysinfo::ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::Always));
    let Some(process) = system.process(pid) else { return Ok(None); };
    let executable = process.exe().filter(|p| !p.as_os_str().is_empty())
        .ok_or("실행 중인 프로세스 경로를 확인할 권한이 없습니다.")?;
    if process.start_time() == 0 { return Err("프로세스 시작 시각을 확인하지 못했습니다.".into()); }
    Ok(Some(ProcessIdentity {pid: pid.as_u32(), start_time: process.start_time(), executable: executable.into()}))
}

pub fn identity_alive(identity: &ProcessIdentity) -> Result<bool, String> {
    // A running image may be renamed by the updater; PID/start identity must still count as alive.
    Ok(process_identity(identity.pid)?.is_some_and(|p| p.start_time == identity.start_time))
}

/// The coordinator supplies an installation-validated identity, never a process-name match.
pub fn terminate_identity(identity: &ProcessIdentity) -> Result<(), String> {
    if !identity_alive(identity)? { return Ok(()); }
    #[cfg(unix)] {
        if unsafe {libc::kill(identity.pid as i32, libc::SIGTERM)} != 0 {
            return Err(format!("확인한 프로세스를 정상 종료하지 못했습니다: {}",std::io::Error::last_os_error()));
        }
    }
    #[cfg(windows)] {
        use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
        use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, TerminateProcess,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE};
        let handle = unsafe {OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,0,identity.pid)};
        if handle.is_null() { return Err(format!("확인한 프로세스 핸들을 열지 못했습니다: {}",std::io::Error::last_os_error())); }
        let result = (|| {
            let mut created: FILETIME = unsafe {std::mem::zeroed()};
            let mut exited = created; let mut kernel = created; let mut user = created;
            if unsafe {GetProcessTimes(handle,&mut created,&mut exited,&mut kernel,&mut user)} == 0 {
                return Err("프로세스 핸들의 시작 시각을 확인하지 못했습니다.".to_string());
            }
            let ticks = (u64::from(created.dwHighDateTime)<<32) | u64::from(created.dwLowDateTime);
            if ticks / 10_000_000 != identity.start_time + 11_644_473_600 {
                return Err("프로세스 신원이 바뀌어 종료를 보류합니다.".to_string());
            }
            if unsafe {TerminateProcess(handle,0)} == 0 {
                return Err(format!("확인한 프로세스 종료 실패: {}",std::io::Error::last_os_error()));
            }
            Ok(())
        })();
        unsafe {CloseHandle(handle)};
        result?;
    }
    Ok(())
}

pub fn canonical_start_allowed(socket: &Path) -> Result<bool, String> {
    canonical_start_allowed_at(&marker_path(), socket, legacy_registration_present, identity_alive)
}

fn canonical_start_allowed_at(
    marker: &Path, socket: &Path,
    legacy: impl FnOnce() -> Result<bool,String>,
    alive: impl FnOnce(&ProcessIdentity) -> Result<bool,String>,
) -> Result<bool,String> {
    if let Some(value) = read_marker_at(marker)? {
        return canonical_start_allowed_for(&value["rename_handover"],socket,alive);
    }
    if crate::is_dept_socket(socket) { return Ok(true); }
    // The published old GUI may restart its legacy job before the new GUI can migrate it.
    Ok(!legacy()?)
}

fn legacy_registration_present() -> Result<bool,String> {
    #[cfg(target_os="macos")] {
        return match crate::launchd::state_for_label(crate::launchd::LEGACY_LAUNCHD_LABEL) {
            crate::launchd::LaunchdState::NotLoaded => Ok(crate::launchd::plist_path()
                .with_file_name(format!("{}.plist",crate::launchd::LEGACY_LAUNCHD_LABEL)).try_exists()
                .map_err(|e|format!("기존 자동 시작 등록 확인 실패: {e}"))?),
            crate::launchd::LaunchdState::Unknown => Err("기존 자동 시작 등록이 불확실해 새 서비스 시작을 보류합니다.".into()),
            _ => Ok(true),
        };
    }
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        let system=std::env::var_os("SystemRoot").ok_or("Windows 시스템 폴더 확인 실패")?;
        let program=PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let script=r#"$ErrorActionPreference='Stop';try{$s=New-Object -ComObject Schedule.Service;$s.Connect();$null=$s.GetFolder('\').GetTask('cysd');'true'}catch{$e=$_.Exception;while($null -ne $e){if($e.HResult -eq -2147024894){'false';exit 0};$e=$e.InnerException};exit 1}"#;
        let output=std::process::Command::new(program).args(["-NoLogo","-NoProfile","-NonInteractive","-Command",script])
            .creation_flags(0x08000000).output().map_err(|e|format!("기존 자동 시작 등록 확인 실패: {e}"))?;
        if !output.status.success(){return Err("기존 작업 스케줄러 등록을 확인하지 못해 새 서비스 시작을 보류합니다.".into());}
        return serde_json::from_slice(&output.stdout).map_err(|_|"기존 작업 스케줄러 등록 응답 확인 실패".into());
    }
    #[cfg(not(any(target_os="macos",windows)))] { Ok(false) }
}

fn canonical_start_allowed_for(
    record: &serde_json::Value,
    socket: &Path,
    alive: impl FnOnce(&ProcessIdentity) -> Result<bool, String>,
) -> Result<bool, String> {
    if record["socket"].as_str() != socket.to_str() { return Ok(true); }
    if record["phase"] == "awaiting-layout" { return Ok(true); }
    if record["phase"] != "starting-new" { return Ok(false); }
    let old = record.get("old_identity")
        .ok_or("이전 프로세스 확인 기록이 없습니다. 자동 시작을 보류합니다.")?;
    match old {
        old if !old.is_null() => {
            let identity: ProcessIdentity = serde_json::from_value(old.clone())
                .map_err(|e| format!("이전 프로세스 확인 기록이 손상됐습니다: {e}"))?;
            if identity.pid == 0 || identity.start_time == 0 || !identity.executable.is_absolute() {
                return Err("이전 프로세스 확인 기록이 올바르지 않습니다. 자동 시작을 보류합니다.".into());
            }
            Ok(!alive(&identity)?)
        }
        _ => Ok(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn fixture(contents: Option<&str>, check: impl FnOnce(&Path)) {
        let raw = std::env::var_os("WAVE_TEST_ROOT").expect("WAVE_TEST_ROOT must be explicitly injected");
        let root = PathBuf::from(raw);
        assert!(root.is_absolute(), "WAVE_TEST_ROOT must be absolute");
        assert!(!root.components().any(|c| matches!(c, std::path::Component::ParentDir)), "WAVE_TEST_ROOT traversal refused");
        let normalized = root.to_string_lossy().replace('\\', "/");
        for protected in ["/Applications", "/opt/homebrew/bin", "/usr/local/bin", "/Library/LaunchAgents", "/.cys"] {
            assert!(!normalized.contains(protected), "WAVE_TEST_ROOT protected path refused");
        }
        #[cfg(unix)] {
            use std::os::unix::fs::MetadataExt;
            assert!(normalized.starts_with("/private/tmp/wave-test-") || normalized.starts_with("/tmp/wave-test-"),
                "WAVE_TEST_ROOT must be a provisioned disposable test directory");
            let mut prefix = PathBuf::new();
            for part in root.components() {
                prefix.push(part.as_os_str());
                let metadata = prefix.symlink_metadata().expect("test root ancestor unavailable");
                assert!(metadata.is_dir() && !metadata.file_type().is_symlink(), "test root ancestor symlink refused");
            }
            let metadata = root.symlink_metadata().unwrap();
            assert_eq!(metadata.uid(), unsafe {libc::geteuid()}, "test root must be owned by test user");
            assert_eq!(metadata.mode() & 0o077, 0, "test root must be private");
        }
        assert!(root.symlink_metadata().unwrap().is_dir(), "WAVE_TEST_ROOT must be a real directory");
        let dir = root.join(format!(
            "wave-handover-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        let marker = dir.join(".pending-restore");
        if let Some(contents) = contents { std::fs::write(&marker, contents).unwrap(); }
        check(&marker);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn startup_guard_excludes_handover_until_owner_releases_same_inode() {
        fixture(None, |marker| {
            let socket = marker.with_file_name("primary.sock");
            let owner = acquire_startup_guard(&socket).unwrap();
            assert!(acquire_startup_guard(&socket).is_err());
            assert!(!marker.exists());
            drop(owner);
            let handover = acquire_startup_guard(&socket).unwrap();
            write_marker_at(marker, Some(&serde_json::json!({"rename_handover":{"phase":"prepared","socket":socket}}))).unwrap();
            assert!(autostart_blocked_at(marker, &socket).unwrap());
            assert!(acquire_startup_guard(&socket).is_err());
            drop(handover);
            assert!(acquire_startup_guard(&socket).is_ok());
        });
    }

    #[test]
    fn legacy_job_cannot_start_new_primary_before_gui_handover() {
        for contents in [None,Some("")] {
            fixture(contents,|marker| {
                assert!(!canonical_start_allowed_at(marker,Path::new("primary"),||Ok(true),|_|panic!("no owner record")).unwrap());
                assert!(canonical_start_allowed_at(marker,Path::new("primary"),||Ok(false),|_|panic!("no owner record")).unwrap());
                assert!(canonical_start_allowed_at(marker,Path::new("primary"),||Err("unknown".into()),|_|panic!("no owner record")).is_err());
            });
        }
        fixture(Some(r#"{"rename_handover":{"phase":"starting-new","socket":"primary","old_identity":null}}"#),|marker| {
            assert!(canonical_start_allowed_at(marker,Path::new("primary"),||panic!("handover authorizes registration transition"),|_|panic!("fresh owner")).unwrap());
        });
        fixture(Some(r#"{"rename_handover":{"phase":"awaiting-layout","socket":"primary"}}"#),|marker| {
            assert!(!autostart_blocked_at(marker,Path::new("primary")).unwrap());
            assert!(canonical_start_allowed_at(marker,Path::new("primary"),||panic!("completed handover"),|_|panic!("completed handover")).unwrap());
        });
    }

    #[test]
    fn absent_and_empty_legacy_markers_do_not_block() {
        for contents in [None, Some(""), Some(" \n")] {
            fixture(contents, |p| assert!(!autostart_blocked_at(p, Path::new("primary")).unwrap()));
        }
    }

    #[test]
    fn active_handover_blocks_only_its_socket() {
        fixture(Some(r#"{"rename_handover":{"phase":"stopping-old","socket":"primary"}}"#), |p| {
            assert!(autostart_blocked_at(p, Path::new("primary")).unwrap());
            assert!(!autostart_blocked_at(p, Path::new("department")).unwrap());
        });
    }

    #[test]
    fn damaged_or_unknown_phase_is_not_permission_to_spawn() {
        for contents in ["{broken", r#"{"rename_handover":{"socket":"primary"}}"#,
            r#"{"rename_handover":{"phase":"unexpected","socket":"primary"}}"#] {
            fixture(Some(contents), |p| assert!(autostart_blocked_at(p, Path::new("primary")).is_err()));
        }
    }

    #[test]
    fn marker_updates_preserve_legacy_restore_and_foreign_temp_files() {
        fixture(Some(""), |p| {
            let value = serde_json::json!({"rename_handover":{"phase":"prepared","socket":"primary"}});
            write_marker_at(p, Some(&value)).unwrap();
            assert!(autostart_blocked_at(p, Path::new("primary")).unwrap());
            write_marker_at(p, None).unwrap();
            assert_eq!(std::fs::read_to_string(p).unwrap(), "");
            let temp = p.parent().unwrap().join(format!(".pending-restore.{}.tmp", std::process::id()));
            std::fs::write(&temp, "KEEP").unwrap();
            assert!(write_marker_at(p, Some(&value)).is_err());
            assert_eq!(std::fs::read_to_string(temp).unwrap(), "KEEP");
        });
    }

    #[cfg(unix)]
    #[test]
    fn marker_symlink_never_reads_or_changes_the_target() {
        fixture(None, |p| {
            let target = p.with_extension("owner-data");
            std::fs::write(&target, "KEEP").unwrap();
            std::os::unix::fs::symlink(&target, p).unwrap();
            assert!(autostart_blocked_at(p, Path::new("primary")).is_err());
            assert!(write_marker_at(p, None).is_err());
            assert_eq!(std::fs::read_to_string(target).unwrap(), "KEEP");
        });
    }

    #[test]
    fn canonical_start_requires_cutover_phase_and_proven_old_exit() {
        let old = ProcessIdentity {pid: 21, start_time: 42, executable: std::env::temp_dir().join("legacy")};
        for phase in ["prepared", "stopping-old", "restoring", "failed"] {
            let record = serde_json::json!({"phase":phase,"socket":"main","old_identity":old});
            assert!(!canonical_start_allowed_for(&record, Path::new("main"), |_| panic!("not allowed to probe/spawn")).unwrap());
        }
        let record = serde_json::json!({"phase":"starting-new","socket":"main","old_identity":old});
        assert!(!canonical_start_allowed_for(&record, Path::new("main"), |_| Ok(true)).unwrap());
        assert!(canonical_start_allowed_for(&record, Path::new("main"), |_| Ok(false)).unwrap());
        assert!(canonical_start_allowed_for(&record, Path::new("main"), |_| Err("unknown".into())).is_err());
        assert!(canonical_start_allowed_for(&record, Path::new("department"), |_| panic!("different socket")).unwrap());
        let missing = serde_json::json!({"phase":"starting-new","socket":"main"});
        assert!(canonical_start_allowed_for(&missing, Path::new("main"), |_| panic!("no identity")).is_err());
        let invalid = serde_json::json!({"phase":"starting-new","socket":"main","old_identity":{
            "pid":0,"start_time":0,"executable":"relative"}});
        assert!(canonical_start_allowed_for(&invalid, Path::new("main"), |_| panic!("invalid identity")).is_err());
    }

    #[test]
    fn process_identity_treats_image_rename_as_same_live_owner() {
        let mut identity = process_identity(std::process::id()).unwrap().unwrap();
        identity.executable = "renamed-by-installer".into();
        assert!(identity_alive(&identity).unwrap());
        identity.start_time += 1;
        assert!(!identity_alive(&identity).unwrap());
    }
}
