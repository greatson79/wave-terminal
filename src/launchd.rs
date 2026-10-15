//! launchd 자동등록 — 앱 첫 실행 시 cysd를 RunAtLoad·KeepAlive로 상시 가동 등록.
//!
//! 수동 `cys daemon install`과 **동일 plist 포맷·경로를 단일 소스**로 공유해
//! 자동/수동 등록의 포맷 드리프트를 막는다. 전체가 macOS 한정.
#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};

pub const LAUNCHD_LABEL: &str = "com.cysjavis.cysd";

/// ~/Library/LaunchAgents/com.cysjavis.cysd.plist
pub fn plist_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist"))
}

/// 데몬 로그 경로(소켓 디렉터리 옆 cysd.log).
pub fn log_path() -> PathBuf {
    crate::socket_path()
        .parent()
        .map(|d| d.join("cysd.log"))
        .unwrap_or_else(|| PathBuf::from("/tmp/cysd.log"))
}

/// XML `<string>` 콘텐츠 이스케이프 — 경로·사용자명에 `&`·`<`·`>`가 있어도
/// plist가 조기 종료·손상되지 않게 엔티티로 치환한다.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// launchd plist 본문 — RunAtLoad(로그인 자동 기동) + KeepAlive(사망 시 재기동).
pub fn render_plist(daemon: &Path, log: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LAUNCHD_LABEL}</string>
  <key>ProgramArguments</key><array><string>{daemon}</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
  <key>ThrottleInterval</key><integer>10</integer>
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#,
        daemon = xml_escape(&daemon.display().to_string()),
        log = xml_escape(&log.display().to_string()),
    )
}

/// launchd가 현재 LAUNCHD_LABEL을 적재 중인가(`launchctl list <label>` 성공 여부).
pub fn is_loaded() -> bool {
    loaded_pid().0
}

/// ★W3: CLI autostart가 launchd에 위임할지 판정(순수 함수). launchd가 cysd 서비스를 적재
/// 중이면 sibling spawn 대신 `launchctl kickstart`로 위임한다 — 구형 CLI가 자기 옆 구형 cysd를
/// 띄워 startup lock을 선점하고 launchd 신형(KeepAlive)과 crashloop 하는 경로를 원천 차단.
pub fn should_delegate_autostart(loaded: bool) -> bool {
    loaded
}

/// ★W3: `launchctl kickstart` 인자(순수 함수 — 테스트 가능). service target = gui/<uid>/<label>.
/// 미가동이면 즉시 기동하고, throttle(crashloop 억제)로 대기 중이어도 강제 기동한다.
pub fn kickstart_args(uid: u32) -> [String; 2] {
    ["kickstart".to_string(), format!("gui/{uid}/{LAUNCHD_LABEL}")]
}

/// launchd에 적재된 cysd 서비스를 kickstart(미가동이면 기동). 성공 여부 반환.
/// current uid로 gui 도메인 타깃을 구성한다(로그인 세션의 사용자 데몬).
pub fn kickstart() -> bool {
    let uid = unsafe { libc::getuid() };
    let args = kickstart_args(uid);
    std::process::Command::new("launchctl")
        .args(args.iter().map(String::as_str))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn kickstart_restart() -> bool {
    let uid = unsafe { libc::getuid() };
    std::process::Command::new("launchctl")
        .args(["kickstart", "-k", &format!("gui/{uid}/{LAUNCHD_LABEL}")])
        .output().map(|o| o.status.success()).unwrap_or(false)
}

fn loaded_pid() -> (bool, Option<u32>) {
    let Ok(out) = std::process::Command::new("launchctl").args(["list", LAUNCHD_LABEL]).output() else {
        return (false, None);
    };
    let pid = String::from_utf8_lossy(&out.stdout).lines().find_map(|line| {
        line.trim().strip_prefix("\"PID\" = ")?.trim_end_matches(';').parse().ok()
    });
    (out.status.success(), pid)
}

pub fn owns_daemon(pid: u32) -> bool {
    let (loaded, launchd_pid) = loaded_pid();
    loaded && launchd_pid == Some(pid)
}

fn plist_matches(daemon: &Path) -> bool {
    std::fs::read_to_string(plist_path()).ok().as_deref() == Some(render_plist(daemon, &log_path()).as_str())
}

/// 이 결과에서 launchd가 cysd 기동을 책임지는가 — 앱 setup이 **수동 spawn을 건너뛰고**
/// launchd-owned cysd의 socket-ready를 폴링할지 결정한다(split-brain·이중 spawn 방지).
/// Registered/AlreadyRegistered = launchd가 띄움. DeferredDaemonRunning = 기존 데몬이
/// 이미 가동(connect로 충분, launchd 미적재).
pub fn launchd_will_serve(outcome: RegisterOutcome) -> bool {
    matches!(
        outcome,
        RegisterOutcome::Registered | RegisterOutcome::AlreadyRegistered
    )
}

/// plist를 ~/Library/LaunchAgents에 기록(부모 디렉터리 생성 포함). 기록한 경로 반환.
pub fn write_plist(daemon: &Path) -> std::io::Result<PathBuf> {
    let path = plist_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, render_plist(daemon, &log_path()))?;
    Ok(path)
}

/// 자동등록 결과 — 호출자가 로그·UI 분기에 사용.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterOutcome {
    /// plist가 이미 존재 — 이미 launchd 소유(무동작).
    AlreadyRegistered,
    /// plist 기록 + launchctl load 완료(launchd가 cysd 소유).
    Registered,
    /// 데몬이 이미 가동 중 — plist만 기록(load 보류, 다음 로그인 발효).
    /// 현 세션의 flock 단일소유를 깨지 않아 가동 세션 소멸을 피한다.
    DeferredDaemonRunning,
}

/// 자동등록 결정(순수 함수 — 부작용 없음, 진리표 테스트 가능).
/// 멱등성은 plist 존재만이 아니라 **'plist 존재 AND launchd 적재'** 교차검사로 판정한다 —
/// DeferredDaemonRunning(plist만 기록·load 보류) 후 데몬이 소멸하면 plist는 남아도
/// 적재는 안 된 상태이므로, 재호출 시 `loaded=false`를 보고 load를 시도해 KeepAlive 보호를 복원한다.
struct Plan {
    write: bool,
    bootout: bool,
    load: bool,
    outcome: RegisterOutcome,
}

/// `fresh` = plist 본문 전체가 현재 render_plist와 일치.
fn plan(fresh: bool, loaded: bool, daemon_running: bool, launchd_pid: Option<u32>, holder_pid: Option<u32>) -> Plan {
    if fresh && loaded {
        // plist가 올바른 경로로 존재 AND launchd 적재 중 — 무동작.
        Plan { write: false, bootout: false, load: false, outcome: RegisterOutcome::AlreadyRegistered }
    } else if (loaded && holder_pid.is_some() && launchd_pid == holder_pid)
        || (daemon_running && (!loaded || holder_pid.is_none())) {
        // shortcut: 주인 PID를 못 읽으면 가동 중인 항목을 보존하고 plist 변경은 다음 로그인에 적용한다.
        Plan { write: !fresh, bootout: false, load: false, outcome: RegisterOutcome::DeferredDaemonRunning }
    } else {
        // 미가동 + (미적재 또는 stale) → 재기록 + load(신규 등록·보호 복원·stale 경로 교정).
        Plan { write: !fresh, bootout: loaded, load: true, outcome: RegisterOutcome::Registered }
    }
}

/// 앱 첫 실행/매 기동 시 자동등록(멱등). 부작용 없는 `plan()`이 결정한 대로 plist 기록·load.
/// 멱등성은 'plist 본문 일치(fresh) AND launchd 적재' 교차검사로 판정한다.
/// - 미등록 + 데몬 미가동(첫 실행): plist write + `launchctl load` → launchd가 즉시 cysd 소유
/// - plist 존재하나 미적재 + 데몬 미가동(deferred 후 데몬 소멸 등): load로 KeepAlive 보호 복원
/// - plist가 옛 경로(stale drift) + 데몬 미가동: 재기록 + reload로 경로 교정
/// - 데몬 가동 중: plist만 보장(load 보류) → 현 세션 flock 단일소유 비파괴
/// - fresh AND 적재 중: 무동작
pub fn register_if_absent(daemon: &Path, daemon_running: bool) -> std::io::Result<RegisterOutcome> {
    // KeepAlive 정책 변경도 본문 불일치로 판정한다.
    let fresh = plist_matches(daemon);
    let (loaded, launchd_pid) = loaded_pid();
    let holder_pid = std::fs::read_to_string(crate::socket_path().with_extension("lock"))
        .ok().and_then(|s| s.trim().parse::<u32>().ok());
    let p = plan(fresh, loaded, daemon_running, launchd_pid, holder_pid);
    if p.write {
        write_plist(daemon)?;
    }
    if p.bootout {
        let uid = unsafe { libc::getuid() };
        let out = std::process::Command::new("launchctl")
            .args(["bootout", &format!("gui/{uid}/{LAUNCHD_LABEL}")]).output()?;
        if !out.status.success() {
            return Err(std::io::Error::other(format!("launchctl bootout failed: {}", String::from_utf8_lossy(&out.stderr).trim())));
        }
    }
    if p.load {
        let path = plist_path();
        let out = std::process::Command::new("launchctl")
            .args(["load", "-w"])
            .arg(&path)
            .output()?;
        if !out.status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "launchctl load failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            ));
        }
    }
    Ok(p.outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn render_plist_has_reboot_survival_keys_and_daemon_path() {
        let daemon = Path::new("/Applications/cys.app/Contents/MacOS/cysd");
        let log = Path::new("/tmp/cysd.log");
        let plist = render_plist(daemon, log);
        // 재부팅 생존 핵심: RunAtLoad(로그인 자동 기동) + KeepAlive(사망 시 재기동).
        assert!(plist.contains("<key>RunAtLoad</key><true/>"), "RunAtLoad 누락");
        assert!(plist.contains("<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>"), "KeepAlive 누락");
        // launchd가 띄울 대상은 정확한 cysd 절대경로.
        assert!(
            plist.contains("<string>/Applications/cys.app/Contents/MacOS/cysd</string>"),
            "ProgramArguments에 cysd 경로 누락"
        );
        assert!(plist.contains(&format!("<string>{LAUNCHD_LABEL}</string>")), "Label 누락");
        assert!(plist.contains("<string>/tmp/cysd.log</string>"), "로그 경로 누락");
        // 유효한 plist 골격.
        assert!(plist.starts_with("<?xml"), "plist 헤더 누락");
    }

    #[test]
    fn label_matches_manual_install_contract() {
        // 수동 `cys daemon install`과 동일 라벨이어야 status/uninstall이 자동등록분을 인식한다.
        assert_eq!(LAUNCHD_LABEL, "com.cysjavis.cysd");
    }

    #[test]
    fn render_plist_escapes_xml_special_chars_in_path() {
        // 사용자명·경로에 &,<,> 가 있어도 plist가 손상되지 않아야 한다.
        let daemon = Path::new("/Users/x/a&b<c>/cys.app/Contents/MacOS/cysd");
        let plist = render_plist(daemon, Path::new("/tmp/l&g.log"));
        assert!(plist.contains("/Users/x/a&amp;b&lt;c&gt;/cys.app/Contents/MacOS/cysd"));
        assert!(plist.contains("/tmp/l&amp;g.log"));
        // 원시(미이스케이프) 앰퍼샌드가 <string> 안에 남으면 안 된다.
        assert!(!plist.contains("a&b<c>"));
    }

    #[test]
    fn single_owner_launchd_plan_truth_table() {
        use RegisterOutcome::*;
        let check = |fresh, loaded, running, launchd_pid, holder_pid| {
            let p = plan(fresh, loaded, running, launchd_pid, holder_pid);
            (p.write, p.bootout, p.load, p.outcome)
        };
        assert_eq!(check(true, true, true, Some(1), Some(2)), (false, false, false, AlreadyRegistered));
        assert_eq!(check(false, false, false, None, None), (true, false, true, Registered));
        assert_eq!(check(true, false, false, None, None), (false, false, true, Registered));
        assert_eq!(check(false, false, true, None, Some(2)), (true, false, false, DeferredDaemonRunning));
        assert_eq!(check(false, true, true, Some(2), Some(2)), (true, false, false, DeferredDaemonRunning));
        assert_eq!(check(false, true, false, Some(2), Some(2)), (true, false, false, DeferredDaemonRunning));
        assert_eq!(check(false, true, true, Some(2), None), (true, false, false, DeferredDaemonRunning));
        assert_eq!(check(false, true, true, None, Some(2)), (true, true, true, Registered));
        assert_eq!(check(false, true, true, Some(1), Some(2)), (true, true, true, Registered));
        assert_eq!(check(false, true, false, Some(1), None), (true, true, true, Registered));
    }

    #[test]
    fn should_delegate_autostart_iff_launchd_loaded() {
        // ★W3 (d): launchd 적재 시 위임(kickstart), 미적재 시 sibling spawn 경로.
        assert!(should_delegate_autostart(true), "적재=위임");
        assert!(!should_delegate_autostart(false), "미적재=sibling");
    }

    #[test]
    fn kickstart_args_targets_gui_domain_label() {
        // ★W3 (d): service target = gui/<uid>/<label> — 로그인 세션 사용자 데몬.
        assert_eq!(
            kickstart_args(501),
            ["kickstart".to_string(), "gui/501/com.cysjavis.cysd".to_string()]
        );
        // uid는 그대로 반영(다른 사용자 오타깃 방지).
        assert_eq!(kickstart_args(0)[1], "gui/0/com.cysjavis.cysd");
    }

    #[test]
    fn launchd_will_serve_only_for_registered_and_already() {
        use RegisterOutcome::*;
        // launchd가 띄울 책임 → 앱 setup이 수동 spawn 건너뛰고 socket-ready 폴링.
        assert!(launchd_will_serve(Registered));
        assert!(launchd_will_serve(AlreadyRegistered));
        // 기존 데몬 가동 중(deferred) → launchd 미적재, connect로 충분 → 수동 경로 허용.
        assert!(!launchd_will_serve(DeferredDaemonRunning));
    }
}
