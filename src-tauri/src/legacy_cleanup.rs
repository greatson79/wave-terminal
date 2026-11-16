//! 0.3.6 C2 — remove the leftover original-cys app (bundle id `com.cysjavis.terminal`) at app start.
//! Matching is by bundle id only (never by name). User data (~/.cys, ~/.local/state/cys, dept dirs) is never touched.
//! Every side effect goes through `Ops`/`Paths` so tests run on an injected root (11/7 rule: never the real /Applications).
#![cfg_attr(not(windows), allow(dead_code))] // Windows matcher/parser are exercised by tests on every OS
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(crate) const LEGACY_BUNDLE_ID: &str = "com.cysjavis.terminal";
pub(crate) const LEGACY_JOB_LABEL: &str = "com.cysjavis.cysd";

/// One Korean line per removal/skip, shown once in the header notice (appended to the migration notice).
static NOTICE: Mutex<Option<String>> = Mutex::new(None);
pub(crate) fn take_notice() -> Option<String> { NOTICE.lock().ok().and_then(|mut n| n.take()) }

pub(crate) trait Ops {
    /// Live processes whose executable lives under `dir`.
    fn processes_under(&self, dir: &Path) -> Vec<(u32, PathBuf)>;
    fn terminate(&mut self, pid: u32) -> Result<(), String>;
    fn file_exists(&self, path: &Path) -> bool { path.exists() }
    fn bootout(&mut self, label: &str) -> Result<(), String>;
    /// Windows: run the original product's own silent uninstaller.
    fn run_uninstaller(&mut self, command: &str) -> Result<(), String>;
    /// ERRATUM 2: every live process with its executable, cwd and argv (in-use and live-seat proofs).
    fn processes(&self) -> Vec<Proc>;
    /// pids holding the shared daemon socket open (`lsof`). Err = unknown -> callers fail closed.
    fn socket_holders(&self, socket: &Path) -> Result<Vec<u32>, String>;
    /// launchd job loaded AND running (`launchctl print gui/<uid>/<label>` state = running). Err = unknown.
    fn job_running(&self, label: &str) -> Result<bool, String>;
    /// The subset of `pids` with an open file under `dir` (`lsof -p`). Err = unknown.
    fn open_files_under(&self, pids: &[u32], dir: &Path) -> Result<Vec<u32>, String>;
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Proc { pub pid: u32, pub exe: Option<PathBuf>, pub cwd: Option<PathBuf>, pub argv: Vec<String> }

pub(crate) struct Paths {
    pub app_roots: Vec<PathBuf>,
    pub launch_agents: PathBuf,
    pub archive: PathBuf,
    pub own_bundle: Option<PathBuf>,
    pub stamp: String,
    /// ERRATUM 2 (C5 gate): the shared pack, the ~/.cys tree seats depend on, the shared socket, and where pack backups go.
    pub pack: PathBuf,
    pub cys_home: PathBuf,
    pub socket: PathBuf,
    pub backup_root: PathBuf,
}

fn bundle_info(bundle: &Path) -> Option<(String, String)> {
    let v = plist::Value::from_file(bundle.join("Contents/Info.plist")).ok()?;
    let d = v.as_dictionary()?;
    let get = |k: &str| d.get(k).and_then(|x| x.as_string()).map(str::to_string);
    Some((get("CFBundleIdentifier")?, get("CFBundleExecutable").unwrap_or_default()))
}

fn plist_program(path: &Path) -> Option<PathBuf> {
    let meta = path.symlink_metadata().ok()?;
    if !meta.is_file() { return None; }
    let v = plist::Value::from_file(path).ok()?;
    let args = v.as_dictionary()?.get("ProgramArguments")?.as_array()?;
    args.first()?.as_string().map(PathBuf::from)
}

/// Leftover original-cys bundles in `p.app_roots` (bundle id match only; symlinks/own bundle excluded). Read-only.
fn find_legacy_apps(p: &Paths) -> Vec<(PathBuf, String, String)> {
    let mut found = Vec::new();
    for root in &p.app_roots {
        let Ok(rd) = std::fs::read_dir(root) else { continue };
        let mut apps: Vec<PathBuf> = rd.flatten().map(|e| e.path())
            .filter(|a| a.extension().is_some_and(|x| x == "app")).collect();
        apps.sort();
        for app in apps {
            let Ok(meta) = app.symlink_metadata() else { continue };
            if meta.file_type().is_symlink() || !meta.is_dir() { continue; }
            let Some((id, exe)) = bundle_info(&app) else { continue };
            if id != LEGACY_BUNDLE_ID { continue; }
            if p.own_bundle.as_ref().is_some_and(|own| own.canonicalize().ok() == app.canonicalize().ok()) { continue; }
            let name = app.file_name().and_then(|n| n.to_str()).unwrap_or("cys.app").to_string();
            found.push((app, name, exe));
        }
    }
    found
}

fn legacy_plist(p: &Paths) -> PathBuf { p.launch_agents.join(format!("{LEGACY_JOB_LABEL}.plist")) }

/// Read-only: what `clean_macos` would remove. Used for the real-shape scan (C6) — touches nothing.
pub(crate) fn plan_macos(p: &Paths) -> Vec<String> {
    let mut out = Vec::new();
    for (app, _, _) in find_legacy_apps(p) {
        out.push(format!("REMOVE app {}", app.display()));
        let plist = legacy_plist(p);
        if let Some(prog) = plist_program(&plist) {
            if prog.starts_with(&app) { out.push(format!("REMOVE plist {}", plist.display())); }
            else { out.push(format!("KEEP plist {} (program {} is not inside the leftover app)", plist.display(), prog.display())); }
        }
    }
    out
}

/// launchd job file -> (label, program): `Program`, else `ProgramArguments[0]`; label = `Label`, else the file stem.
fn plist_job(path: &Path) -> Option<(String, PathBuf)> {
    let meta = path.symlink_metadata().ok()?;
    if !meta.is_file() { return None; }
    let v = plist::Value::from_file(path).ok()?;
    let d = v.as_dictionary()?;
    let prog = d.get("Program").and_then(|x| x.as_string())
        .or_else(|| d.get("ProgramArguments")?.as_array()?.first()?.as_string())?;
    let label = d.get("Label").and_then(|x| x.as_string()).map(str::to_string)
        .or_else(|| path.file_stem().and_then(|s| s.to_str()).map(str::to_string))?;
    Some((label, PathBuf::from(prog)))
}

/// ERRATUM 2 (C2 amended): the bundle id is necessary, not sufficient. `Some(reason)` = the old fork is in use, leave it.
/// Any signal we cannot read counts as "in use" (fail closed).
fn old_fork_in_use(app: &Path, p: &Paths, ops: &dyn Ops, procs: &[Proc]) -> Option<&'static str> {
    let inside = |x: &Path| x.starts_with(app);
    // (1) a running process's executable is inside the bundle (GUI or daemon)
    if procs.iter().any(|pr| pr.exe.as_deref().is_some_and(inside)) { return Some("실행 중인 프로그램"); }
    // (2) the daemon holding the shared socket was started from inside it (owner exe unknown = fail closed)
    match ops.socket_holders(&p.socket) {
        Err(_) => return Some("공유 소켓 확인 불가"),
        Ok(holders) => {
            for pid in holders {
                match procs.iter().find(|pr| pr.pid == pid).and_then(|pr| pr.exe.as_deref()) {
                    Some(exe) if !inside(exe) => {}
                    _ => return Some("공유 소켓을 쓰는 데몬"),
                }
            }
        }
    }
    // (3) a launchd job (any label) whose Program/ProgramArguments points into it AND is loaded/running (unknown = running)
    if let Ok(rd) = std::fs::read_dir(&p.launch_agents) {
        for e in rd.flatten() {
            if e.path().extension().is_none_or(|x| x != "plist") { continue; }
            let Some((label, prog)) = plist_job(&e.path()) else { continue };
            if inside(&prog) && ops.job_running(&label).unwrap_or(true) { return Some("실행 중인 자동 시작 작업"); }
        }
    }
    // (4) a live seat's cwd or argv references it
    let needle = app.display().to_string();
    if procs.iter().any(|pr| pr.cwd.as_deref().is_some_and(inside) || pr.argv.iter().any(|a| a.contains(&needle))) {
        return Some("작업 좌석");
    }
    None
}

pub(crate) fn clean_macos(p: &Paths, ops: &mut dyn Ops) -> Vec<String> { clean_macos_counted(p, ops).0 }

/// (lines, number of leftover bundles actually moved to the archive in this run).
fn clean_macos_counted(p: &Paths, ops: &mut dyn Ops) -> (Vec<String>, usize) {
    let (mut lines, mut removed) = (Vec::new(), 0);
    let apps = find_legacy_apps(p);
    let procs = if apps.is_empty() { Vec::new() } else { ops.processes() };
    for (app, name, _exe) in apps {
        // ERRATUM 2: an old fork in use is left in place (notice only) — nothing is stopped, nothing is removed.
        if let Some(why) = old_fork_in_use(&app, p, ops, &procs) {
            lines.push(format!("남은 cys 앱({name})이 사용 중이라({why}) 그대로 두었습니다."));
            continue;
        }
        let plist = legacy_plist(p);
        if plist_program(&plist).is_some_and(|prog| prog.starts_with(&app)) {
            let _ = ops.bootout(LEGACY_JOB_LABEL); // proven not running above; the file removal below is what matters
            match std::fs::remove_file(&plist) {
                Ok(()) => lines.push(format!("남은 cys 자동 시작 등록({LEGACY_JOB_LABEL})을 정리했습니다.")),
                Err(e) => { lines.push(format!("cys 자동 시작 등록을 지우지 못해 앱도 그대로 둡니다: {e}")); continue; }
            }
        }
        let dest = p.archive.join(format!("{}-{name}", p.stamp));
        let moved = std::fs::create_dir_all(&p.archive).and_then(|_| std::fs::rename(&app, &dest));
        match moved {
            Ok(()) => { removed += 1; lines.push(format!("남은 cys 앱({name})을 보관 폴더로 옮겼습니다: {}", dest.display())); }
            Err(e) => lines.push(format!("남은 cys 앱({name})을 옮기지 못했습니다(권한 등). 그대로 둡니다: {e}")),
        }
    }
    (lines, removed)
}

const SEAT_NAMES: [&str; 4] = ["cysd", "waved", "claude", "codex"];

/// ERRATUM 2 (C5 gate ii): `Some(reason)` = a live seat depends on the pack: anything holding the shared socket, or a
/// running cysd/waved/claude/codex process whose cwd/argv/open files are under ~/.cys. Unreadable = live (fail closed).
fn live_seat(p: &Paths, ops: &dyn Ops) -> Option<&'static str> {
    match ops.socket_holders(&p.socket) {
        Ok(h) if h.is_empty() => {}
        Ok(_) => return Some("공유 소켓 사용 중"),
        Err(_) => return Some("좌석 확인 불가"),
    }
    let base = |s: &str| Path::new(s).file_name().and_then(|n| n.to_str()).map(str::to_string).unwrap_or_default();
    let procs = ops.processes();
    let seats: Vec<&Proc> = procs.iter().filter(|pr| {
        pr.exe.as_deref().and_then(|e| e.file_name()).and_then(|n| n.to_str()).is_some_and(|n| SEAT_NAMES.contains(&n))
            || pr.argv.iter().take(2).any(|a| SEAT_NAMES.contains(&base(a).as_str()))
    }).collect();
    let home = p.cys_home.display().to_string();
    if seats.iter().any(|pr| pr.cwd.as_deref().is_some_and(|c| c.starts_with(&p.cys_home)) || pr.argv.iter().any(|a| a.contains(&home))) {
        return Some("작업 좌석");
    }
    let pids: Vec<u32> = seats.iter().map(|pr| pr.pid).collect();
    match ops.open_files_under(&pids, &p.cys_home) {
        Ok(v) if v.is_empty() => None,
        Ok(_) => Some("작업 좌석이 연 파일"),
        Err(_) => Some("좌석 확인 불가"),
    }
}

/// Recursive copy for the pack backup (files, dirs; symlinks recreated as symlinks on unix). Never deletes anything.
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let (from, to, ft) = (e.path(), dst.join(e.file_name()), e.file_type()?);
        if ft.is_dir() { copy_tree(&from, &to)?; }
        else if ft.is_symlink() { #[cfg(unix)] std::os::unix::fs::symlink(std::fs::read_link(&from)?, &to)?; }
        else { std::fs::copy(&from, &to)?; }
    }
    Ok(())
}

pub(crate) const HEAL_SKIPPED_NO_REMOVAL: &str = "공유 지침 폴더에 원본 cys 지침이 보이지만 이번 실행에서 정리한 원본 cys 앱이 없어 그대로 둡니다.";

/// ERRATUM 2 (C5 amended): heal ONLY when (i) a leftover original app was removed in this same run AND (ii) no live seat
/// depends on the pack; first a timestamped full backup of the pack (never deleted). Otherwise nothing + one notice.
fn heal_gated(removed: usize, p: &Paths, ops: &dyn Ops, heal: &mut dyn FnMut() -> Result<Option<String>, String>) -> Vec<String> {
    if !cys::pack::foreign_pack_drift(&p.pack) { return vec![]; }
    if removed == 0 { return vec![format!("{LOG_ONLY}{HEAL_SKIPPED_NO_REMOVAL}")]; }   // the app line is the one notice
    if let Some(why) = live_seat(p, ops) {
        return vec![format!("공유 지침 폴더에 원본 cys 지침이 보이지만 작업 좌석이 쓰는 중이라({why}) 그대로 둡니다.")];
    }
    let backup = p.backup_root.join(format!("{}-pack", p.stamp));
    if let Err(e) = copy_tree(&p.pack, &backup) { return vec![format!("지침 폴더를 백업하지 못해 그대로 둡니다: {e}")]; }
    match heal() {
        Ok(Some(l)) => vec![format!("{l} (이전 지침 전체 백업: {})", backup.display())],
        Ok(None) => vec![],
        Err(e) => vec![format!("지침 폴더를 되돌리지 못했습니다(이전 지침 백업: {}): {e}", backup.display())],
    }
}

/// Start sequence on macOS: leftover-app cleanup, then the gated C5 pack heal (same background thread, same run).
pub(crate) fn clean_and_heal_macos(p: &Paths, ops: &mut dyn Ops, heal: &mut dyn FnMut() -> Result<Option<String>, String>) -> Vec<String> {
    let (mut lines, removed) = clean_macos_counted(p, ops);
    lines.extend(heal_gated(removed, p, ops, heal));
    lines
}

/// One Uninstall registry product entry (HKCU/HKLM `...\Uninstall\<key>`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct UninstallEntry {
    pub key: String,
    pub display_name: String,
    pub publisher: String,
    pub install_location: String,
    pub uninstall_string: String,
}

/// Tauri NSIS names the Uninstall entry after `productName`; the original product's productName is "cys"
/// (upstream tauri.conf.json, W-CYS_EVIDENCE/upstream-conf.json). The real Windows registry values were NOT observed
/// on a machine — see REPORT C9. Wave's own entry is "Wave Terminal".
const LEGACY_DISPLAY_NAME: &str = "cys";
const LEGACY_EXE: &str = "cys-app.exe";

/// Values of the original product's install, OBSERVED on a real Windows runner (official upstream cys 0.14.56 setup,
/// `reg query` dump, run 43880653708 on greatson79/wave-terminal-test; evidence `EVIDENCE/win_legacy_observe/`):
/// Uninstall subkey `cys`, DisplayName `cys`, Publisher `cysjavis`, InstallLocation `"C:\Users\<u>\AppData\Local\cys"`
/// (quoted), UninstallString `"<InstallLocation>\uninstall.exe"` (quoted), MainBinaryName `cys-app.exe`.
pub(crate) struct Observed { pub publisher: &'static str }
pub(crate) const OBSERVED: Option<Observed> = Some(Observed { publisher: "cysjavis" });

/// `"C:\x\uninstall.exe"` (as stored in the registry) -> the bare path. Unquoted values are returned as-is.
pub(crate) fn uninstaller_path(command: &str) -> PathBuf {
    let c = command.trim();
    match c.strip_prefix('"') { Some(rest) => PathBuf::from(rest.split('"').next().unwrap_or("")), None => PathBuf::from(c) }
}

/// Parse `reg query <Uninstall root> /s` output into entries (pure; tested on fixtures).
pub(crate) fn parse_reg_query(text: &str) -> Vec<UninstallEntry> {
    let mut out: Vec<UninstallEntry> = Vec::new();
    for line in text.lines() {
        if line.starts_with("HKEY_") {
            out.push(UninstallEntry { key: line.trim().to_string(), ..Default::default() });
            continue;
        }
        let Some(cur) = out.last_mut() else { continue };
        let t = line.trim();
        let Some(name) = t.split_whitespace().next() else { continue };
        let rest = t[name.len()..].trim_start();
        let Some(kind) = rest.split_whitespace().next().filter(|k| k.starts_with("REG_")) else { continue };
        let value = rest[kind.len()..].trim().to_string();
        match name {
            "DisplayName" => cur.display_name = value,
            "Publisher" => cur.publisher = value,
            "InstallLocation" => cur.install_location = value.trim_matches('"').to_string(),
            "UninstallString" => cur.uninstall_string = value,
            _ => {}
        }
    }
    out
}

fn same_dir(a: &str, b: &Path) -> bool {
    let norm = |s: &str| s.trim_end_matches(['\\', '/']).replace('/', "\\").to_lowercase();
    norm(a) == norm(&b.display().to_string())
}

/// NSIS `uninstall.exe /S` copies itself to %TEMP% and returns at once (observed: run 43883087608 — success reported while
/// `cys-app.exe` and the Uninstall key were still there 5 s later). Poll until `marker` is gone; false = still there.
pub(crate) fn wait_gone(marker: &Path, max_secs: u64) -> bool {
    for _ in 0..max_secs.max(1) {
        if !marker.exists() { return true; }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    !marker.exists()
}

/// Static (cheap, read-only) part of the Windows match: everything except "is a process running / terminate / uninstall".
fn is_windows_candidate(e: &UninstallEntry, own_dir: &Path, obs: &Observed, ops: &dyn Ops) -> bool {
    let key_name = e.key.rsplit(['\\', '/']).next().unwrap_or("");
    key_name.eq_ignore_ascii_case(LEGACY_DISPLAY_NAME) && e.display_name == LEGACY_DISPLAY_NAME
        && e.publisher == obs.publisher && !e.install_location.is_empty() && !e.uninstall_string.is_empty()
        && !same_dir(&e.install_location, own_dir)
        && ops.file_exists(&PathBuf::from(&e.install_location).join(LEGACY_EXE))
}

/// How many entries `clean_windows` would act on (used to decide whether a background cleanup is needed at all).
pub(crate) fn windows_candidates(entries: &[UninstallEntry], own_dir: &Path, observed: Option<&Observed>, ops: &dyn Ops) -> usize {
    observed.map_or(0, |obs| entries.iter().filter(|e| is_windows_candidate(e, own_dir, obs, ops)).count())
}

/// Lines starting with `LOG_ONLY` go to the log file but not to the user notice.
pub(crate) const LOG_ONLY: &str = "[log-only] ";

/// Windows removal needs ALL of: Uninstall subkey name "cys", DisplayName "cys", Publisher == the observed one,
/// `cys-app.exe` inside InstallLocation, and InstallLocation != our own dir. With `observed == None` it only logs.
pub(crate) fn clean_windows(entries: &[UninstallEntry], own_dir: &Path, observed: Option<&Observed>, ops: &mut dyn Ops) -> Vec<String> {
    clean_windows_counted(entries, own_dir, observed, ops).0
}

/// (lines, number of products whose own uninstaller finished in this run).
fn clean_windows_counted(entries: &[UninstallEntry], own_dir: &Path, observed: Option<&Observed>, ops: &mut dyn Ops) -> (Vec<String>, usize) {
    let (mut lines, mut removed) = (Vec::new(), 0);
    for e in entries {
        let key_name = e.key.rsplit(['\\', '/']).next().unwrap_or("");
        if !key_name.eq_ignore_ascii_case(LEGACY_DISPLAY_NAME) || e.display_name != LEGACY_DISPLAY_NAME { continue; }
        let Some(obs) = observed else {
            lines.push(format!("{LOG_ONLY}Windows 제거 비활성(원본 설치값 미관측): 후보 {} / {}", e.key, e.install_location));
            continue;
        };
        if !is_windows_candidate(e, own_dir, obs, ops) { continue; }
        let dir = PathBuf::from(&e.install_location);
        let procs = ops.processes_under(&dir);
        let is_gui = |p: &Path| { let s = p.display().to_string().to_lowercase(); s.ends_with("\\cys-app.exe") || s.ends_with("/cys-app.exe") };
        if procs.iter().any(|(_, exe)| !is_gui(exe)) {
            lines.push("남은 cys 앱의 서비스가 실행 중이라 건드리지 않았습니다.".to_string());
            continue;
        }
        if let Some((pid, _)) = procs.first() {
            if let Err(err) = ops.terminate(*pid) { lines.push(format!("남은 cys 앱을 종료하지 못해 그대로 둡니다: {err}")); continue; }
        }
        match ops.run_uninstaller(&e.uninstall_string) {
            Ok(()) => { removed += 1; lines.push(format!("남은 cys 앱({})을 제거했습니다.", e.install_location)); }
            Err(err) => lines.push(format!("남은 cys 앱을 제거하지 못해 그대로 둡니다(권한 등): {err}")),
        }
    }
    (lines, removed)
}

fn write_log(settings_file: &Path, lines: &[String]) {
    use std::io::Write;
    let Some(dir) = settings_file.parent() else { return };
    let _ = std::fs::create_dir_all(dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("legacy-cleanup.log")) {
        for l in lines { let _ = writeln!(f, "{l}"); }
    }
}

/// Every external program the Mac cleanup runs goes through here. In a test build it REFUSES (fail closed): unit tests
/// and rigs must use stubbed `Ops`, never the real launchctl/lsof of this machine (ERRATUM 2 hard rule).
#[cfg(target_os = "macos")]
fn external(program: &str, args: &[&str]) -> Result<std::process::Output, String> {
    if cfg!(test) { return Err(format!("test guard: real {program} refused")); }
    std::process::Command::new(program).args(args).output().map_err(|e| e.to_string())
}

/// `lsof -t` prints one pid per line; exit 1 with no output = nobody (not an error).
#[cfg(target_os = "macos")]
fn lsof_pids(args: &[&str]) -> Result<Vec<u32>, String> {
    let out = external("/usr/sbin/lsof", args)?;
    let pids: Vec<u32> = String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.trim().parse().ok()).collect();
    if !out.status.success() && pids.is_empty() && !String::from_utf8_lossy(&out.stderr).trim().is_empty() {
        return Err("lsof 확인 실패".into());
    }
    Ok(pids)
}

#[cfg(any(target_os = "macos", windows))]
struct RealOps;
#[cfg(any(target_os = "macos", windows))]
impl Ops for RealOps {
    fn processes_under(&self, dir: &Path) -> Vec<(u32, PathBuf)> {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true,
            sysinfo::ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::Always));
        sys.processes().iter().filter_map(|(pid, p)| p.exe().filter(|e| e.starts_with(dir)).map(|e| (pid.as_u32(), e.to_path_buf()))).collect()
    }
    fn terminate(&mut self, pid: u32) -> Result<(), String> {
        let mut sys = sysinfo::System::new();
        let pid_s = sysinfo::Pid::from_u32(pid);
        sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::Some(&[pid_s]), true, sysinfo::ProcessRefreshKind::nothing());
        let Some(p) = sys.process(pid_s) else { return Ok(()) };
        #[cfg(unix)] { if p.kill_with(sysinfo::Signal::Term).is_none() { return Err("종료 신호를 보내지 못했습니다".into()); } }
        #[cfg(not(unix))] { p.kill(); }
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::Some(&[pid_s]), true, sysinfo::ProcessRefreshKind::nothing());
            if sys.process(pid_s).is_none() { return Ok(()); }
        }
        Err("5초 안에 종료되지 않았습니다".into())
    }
    fn bootout(&mut self, label: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")] {
            let target = format!("gui/{}/{}", unsafe { libc::getuid() }, label);
            let out = external("/bin/launchctl", &["bootout", &target])?;
            if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
        }
        #[cfg(not(target_os = "macos"))] { let _ = label; Ok(()) }
    }
    fn processes(&self) -> Vec<Proc> {
        let mut sys = sysinfo::System::new();
        sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, sysinfo::ProcessRefreshKind::nothing()
            .with_exe(sysinfo::UpdateKind::Always).with_cwd(sysinfo::UpdateKind::Always).with_cmd(sysinfo::UpdateKind::Always));
        sys.processes().iter().map(|(pid, p)| Proc { pid: pid.as_u32(), exe: p.exe().map(Path::to_path_buf),
            cwd: p.cwd().map(Path::to_path_buf), argv: p.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect() }).collect()
    }
    fn socket_holders(&self, socket: &Path) -> Result<Vec<u32>, String> {
        #[cfg(target_os = "macos")] {
            if !socket.exists() { return Ok(vec![]); }   // no socket file = nobody holds it
            lsof_pids(&["-t", "--", &socket.display().to_string()])
        }
        #[cfg(not(target_os = "macos"))] { let _ = socket; Err("이 운영체제에서는 공유 소켓 점유를 확인할 수 없습니다".into()) }
    }
    fn job_running(&self, label: &str) -> Result<bool, String> {
        #[cfg(target_os = "macos")] {
            let target = format!("gui/{}/{}", unsafe { libc::getuid() }, label);
            let out = external("/bin/launchctl", &["print", &target])?;
            if out.status.success() { return Ok(String::from_utf8_lossy(&out.stdout).lines().any(|l| l.trim() == "state = running")); }
            if out.status.code() == Some(113) { return Ok(false); }   // "Could not find service" = not loaded
            Err(format!("launchctl print 확인 실패({:?})", out.status.code()))
        }
        #[cfg(not(target_os = "macos"))] { let _ = label; Ok(false) }
    }
    fn open_files_under(&self, pids: &[u32], dir: &Path) -> Result<Vec<u32>, String> {
        if pids.is_empty() { return Ok(vec![]); }
        #[cfg(target_os = "macos")] {
            let list = pids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
            let out = external("/usr/sbin/lsof", &["-a", "-p", &list, "-Fpn"])?;
            let (text, mut pid, mut hits) = (String::from_utf8_lossy(&out.stdout).into_owned(), 0u32, Vec::new());
            if !out.status.success() && text.trim().is_empty() && !out.stderr.is_empty() { return Err("lsof 확인 실패".into()); }
            for line in text.lines() {
                if let Some(p) = line.strip_prefix('p') { pid = p.parse().unwrap_or(0); }
                else if let Some(n) = line.strip_prefix('n') { if Path::new(n).starts_with(dir) && !hits.contains(&pid) { hits.push(pid); } }
            }
            Ok(hits)
        }
        #[cfg(not(target_os = "macos"))] { let _ = dir; Err("이 운영체제에서는 열린 파일을 확인할 수 없습니다".into()) }
    }
    fn run_uninstaller(&mut self, command: &str) -> Result<(), String> {
        #[cfg(windows)] {
            let exe = uninstaller_path(command);
            let out = std::process::Command::new(&exe).arg("/S").output().map_err(|e| e.to_string())?;
            if !out.status.success() { return Err(format!("종료 코드 {:?}", out.status.code())); }
            // The launcher returned; the real uninstall runs on in a temp copy. Success only once the app is gone.
            let gone = exe.parent().map(|d| wait_gone(&d.join(LEGACY_EXE), 90)).unwrap_or(false);
            if gone { Ok(()) } else { Err("제거 프로그램을 실행했지만 90초 안에 끝났는지 확인하지 못했습니다(진행 중일 수 있습니다)".to_string()) }
        }
        #[cfg(not(windows))] { let _ = command; Ok(()) }
    }
}

/// App-start entry (called once, from `rename_migration::run`). Fail-open: never returns an error, never blocks startup.
/// Theo ruling 11/16: the leftover cleanup must NOT block app start (the Windows uninstaller wait alone can take 90 s).
/// Sets a one-line progress notice at once, runs `work` on a background thread, then writes the TRUTHFUL result lines
/// (including "could not confirm") to the log file and, if the UI asks later, to the notice.
pub(crate) fn run_in_background(settings_file: PathBuf, progress: String, work: impl FnOnce() -> Vec<String> + Send + 'static) -> std::thread::JoinHandle<()> {
    if let Ok(mut n) = NOTICE.lock() { *n = Some(progress); }
    std::thread::spawn(move || {
        let lines = work();
        if lines.is_empty() { return; }
        write_log(&settings_file, &lines);
        let shown: Vec<&str> = lines.iter().filter(|l| !l.starts_with(LOG_ONLY)).map(String::as_str).collect();
        if !shown.is_empty() { if let Ok(mut n) = NOTICE.lock() { *n = Some(shown.join(" · ")); } }
    })
}

#[cfg(any(target_os = "macos", windows))]
const PROGRESS_LINE: &str = "남은 원본 cys 앱을 정리하는 중입니다(백그라운드). 앱은 그대로 쓰실 수 있고, 결과는 legacy-cleanup.log 에 기록됩니다.";

pub(crate) fn run_at_start(settings_file: &Path) -> Option<std::thread::JoinHandle<()>> {
    #[cfg(target_os = "macos")] {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        if home.as_os_str().is_empty() { return None; }
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0).to_string();
        let own_bundle = std::env::current_exe().ok().and_then(|e| e.ancestors().find(|a| a.extension().is_some_and(|x| x == "app")).map(Path::to_path_buf));
        let paths = Paths { app_roots: vec![PathBuf::from("/Applications"), home.join("Applications")],
            launch_agents: home.join("Library/LaunchAgents"),
            archive: settings_file.parent().map(|d| d.join("removed-cys-apps")).unwrap_or_else(|| home.join("removed-cys-apps")),
            own_bundle, stamp, pack: cys::pack::pack_dir(), cys_home: home.join(".cys"), socket: cys::socket_path(),
            backup_root: settings_file.parent().map(|d| d.join("pack-backups")).unwrap_or_else(|| home.join("pack-backups")) };
        if plan_macos(&paths).is_empty() {   // read-only scan: nothing leftover = no thread; C5 gate (i) fails -> notice only
            if cys::pack::foreign_pack_drift(&paths.pack) {
                write_log(settings_file, &[HEAL_SKIPPED_NO_REMOVAL.to_string()]);
                if let Ok(mut n) = NOTICE.lock() { *n = Some(HEAL_SKIPPED_NO_REMOVAL.to_string()); }
            }
            return None;
        }
        return Some(run_in_background(settings_file.to_path_buf(), PROGRESS_LINE.to_string(),
            move || clean_and_heal_macos(&paths, &mut RealOps, &mut cys::pack::heal_foreign_pack)));
    }
    #[cfg(windows)] {
        let mut text = String::new();
        for root in [r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall", r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall"] {
            if let Ok(o) = std::process::Command::new("reg").args(["query", root, "/s"]).output() {
                text.push_str(&String::from_utf8_lossy(&o.stdout)); text.push('\n');
            }
        }
        let entries = parse_reg_query(&text);
        let own = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)).unwrap_or_default();
        if windows_candidates(&entries, &own, OBSERVED.as_ref(), &RealOps) == 0 {
            let lines = clean_windows(&entries, &own, OBSERVED.as_ref(), &mut RealOps);   // log-only sightings; no slow work possible
            if !lines.is_empty() { write_log(settings_file, &lines); }
            return None;
        }
        let home = PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default());
        let paths = Paths { app_roots: vec![], launch_agents: PathBuf::new(), archive: PathBuf::new(), own_bundle: None,
            stamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0).to_string(),
            pack: cys::pack::pack_dir(), cys_home: home.join(".cys"), socket: cys::socket_path(),
            backup_root: settings_file.parent().map(|d| d.join("pack-backups")).unwrap_or_else(|| home.join("pack-backups")) };
        return Some(run_in_background(settings_file.to_path_buf(), PROGRESS_LINE.to_string(), move || {
            let (mut lines, removed) = clean_windows_counted(&entries, &own, OBSERVED.as_ref(), &mut RealOps);
            lines.extend(heal_gated(removed, &paths, &RealOps, &mut cys::pack::heal_foreign_pack));   // Windows: seat proof unavailable -> fail closed (notice)
            lines
        }));
    }
    #[allow(unreachable_code)] { let _ = settings_file; None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;

    #[derive(Default)]
    struct Fake { running: Vec<(u32, PathBuf)>, events: Vec<String>, terminate_fail: bool, plist: Option<PathBuf>, no_files: bool,
        procs: Vec<Proc>, holders: Vec<u32>, loaded: Vec<String>, open_under: Vec<u32> }
    impl Ops for Fake {
        fn processes(&self) -> Vec<Proc> {
            self.running.iter().map(|(pid, e)| Proc { pid: *pid, exe: Some(e.clone()), ..Default::default() }).chain(self.procs.clone()).collect()
        }
        fn socket_holders(&self, _: &Path) -> Result<Vec<u32>, String> { Ok(self.holders.clone()) }
        fn job_running(&self, label: &str) -> Result<bool, String> { Ok(self.loaded.iter().any(|l| l == label)) }
        fn open_files_under(&self, pids: &[u32], _: &Path) -> Result<Vec<u32>, String> {
            Ok(pids.iter().copied().filter(|p| self.open_under.contains(p)).collect())
        }
        fn processes_under(&self, dir: &Path) -> Vec<(u32, PathBuf)> {   // same table as `processes()` (one consistent fake)
            self.processes().into_iter().filter_map(|p| p.exe.filter(|e| e.display().to_string().starts_with(&dir.display().to_string())).map(|e| (p.pid, e))).collect()
        }
        fn terminate(&mut self, pid: u32) -> Result<(), String> {
            self.events.push(format!("terminate {pid}"));
            if self.terminate_fail { Err("거부".into()) } else { Ok(()) }
        }
        fn file_exists(&self, _: &Path) -> bool { !self.no_files }
        fn run_uninstaller(&mut self, command: &str) -> Result<(), String> { self.events.push(format!("uninstall {command}")); Ok(()) }
        fn bootout(&mut self, label: &str) -> Result<(), String> {
            let present = self.plist.as_ref().is_some_and(|p| p.exists());
            self.events.push(format!("bootout {label} plist_present={present}")); Ok(())
        }
    }

    fn info(id: &str, exe: &str) -> String {
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{id}</string><key>CFBundleExecutable</key><string>{exe}</string></dict></plist>"#)
    }
    fn make_app(root: &Path, name: &str, id: &str, exe: &str) -> PathBuf {
        let a = root.join(name);
        fs::create_dir_all(a.join("Contents/MacOS")).unwrap();
        fs::write(a.join("Contents/Info.plist"), info(id, exe)).unwrap();
        fs::write(a.join("Contents/MacOS").join(exe), "bin").unwrap();
        a
    }
    fn make_job(la: &Path, label: &str, program: &Path) {
        fs::create_dir_all(la).unwrap();
        fs::write(la.join(format!("{label}.plist")), format!(r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
<key>Label</key><string>{label}</string><key>ProgramArguments</key><array><string>{}</string></array></dict></plist>"#, program.display())).unwrap();
    }
    fn rig(tag: &str) -> (PathBuf, Paths) {
        let t = std::env::temp_dir().join(format!("wave-lc-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let (apps, home_apps) = (t.join("Applications"), t.join("home/Applications"));
        fs::create_dir_all(&apps).unwrap(); fs::create_dir_all(&home_apps).unwrap();
        let p = Paths { app_roots: vec![apps, home_apps], launch_agents: t.join("home/Library/LaunchAgents"),
            archive: t.join("cfg/removed-cys-apps"), own_bundle: None, stamp: "T1".into(),
            pack: t.join("home/.cys/pack"), cys_home: t.join("home/.cys"), socket: t.join("home/.local/state/cys/cys.sock"),
            backup_root: t.join("cfg/pack-backups") };
        (t, p)
    }
    /// path -> content hash for every file/dir under dirs that must stay byte-identical.
    fn snapshot(dirs: &[&Path]) -> BTreeMap<String, String> {
        fn walk(d: &Path, out: &mut BTreeMap<String, String>) {
            for e in fs::read_dir(d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() { out.insert(p.display().to_string(), "<dir>".into()); walk(&p, out); }
                else { out.insert(p.display().to_string(), format!("{:?}", fs::read(&p).unwrap())); }
            }
        }
        let mut m = BTreeMap::new();
        for d in dirs { walk(d, &mut m); }
        m
    }
    fn sentinels(t: &Path) -> Vec<PathBuf> {
        let ds = vec![t.join("home/.cys"), t.join("home/.local/state/cys"), t.join("home/dept/목회")];
        for d in &ds { fs::create_dir_all(d).unwrap(); fs::write(d.join("record.json"), format!("{{\"k\":\"{}\"}}", d.display())).unwrap(); }
        ds
    }

    #[test]
    fn removes_only_bundle_id_match_boots_out_before_plist_delete_and_leaves_user_data_identical() {
        let (t, p) = rig("main");
        let cys = make_app(&p.app_roots[0], "cys.app", LEGACY_BUNDLE_ID, "cys-app");
        make_job(&p.launch_agents, LEGACY_JOB_LABEL, &cys.join("Contents/MacOS/cysd"));
        let wave = make_app(&p.app_roots[0], "Wave Terminal.app", "com.waveainetworks.wave-terminal", "wave-app");
        let named_cys_other_id = make_app(&p.app_roots[1], "cys.app", "com.someone.else", "x");
        let ds = sentinels(&t);
        let refs: Vec<&Path> = ds.iter().map(|d| d.as_path()).collect();
        let before = snapshot(&refs);
        // ERRATUM 2: a running GUI now means "in use" (left alone, see erratum2_old_fork_in_use_signals_each_keep_the_app);
        // here nothing of the leftover app runs and its job is not loaded.
        let mut ops = Fake { plist: Some(p.launch_agents.join(format!("{LEGACY_JOB_LABEL}.plist"))), ..Default::default() };

        let lines = clean_macos(&p, &mut ops);

        assert_eq!(ops.events, [format!("bootout {LEGACY_JOB_LABEL} plist_present=true")]);
        assert!(!cys.exists() && !p.launch_agents.join(format!("{LEGACY_JOB_LABEL}.plist")).exists());
        assert!(p.archive.join("T1-cys.app/Contents/Info.plist").exists(), "archived, not destroyed");
        assert!(wave.exists() && named_cys_other_id.exists(), "other id / our own bundle untouched");
        assert_eq!(snapshot(&refs), before, "user data byte-identical");
        assert_eq!(lines.len(), 2, "{lines:?}");
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn plist_pointing_at_our_own_bundle_is_left_alone() {
        let (t, p) = rig("ownplist");
        let cys = make_app(&p.app_roots[0], "cys.app", LEGACY_BUNDLE_ID, "cys-app");
        let wave = make_app(&p.app_roots[0], "Wave Terminal.app", "com.waveainetworks.wave-terminal", "wave-app");
        make_job(&p.launch_agents, LEGACY_JOB_LABEL, &wave.join("Contents/MacOS/cysd"));
        let mut ops = Fake::default();
        clean_macos(&p, &mut ops);
        assert!(!cys.exists());
        assert!(p.launch_agents.join(format!("{LEGACY_JOB_LABEL}.plist")).exists(), "Wave-owned registration stays for the rename migration");
        assert!(ops.events.is_empty());
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn running_daemon_from_the_leftover_app_is_never_stopped_and_nothing_is_removed() {
        let (t, p) = rig("daemon");
        let cys = make_app(&p.app_roots[0], "cys.app", LEGACY_BUNDLE_ID, "cys-app");
        make_job(&p.launch_agents, LEGACY_JOB_LABEL, &cys.join("Contents/MacOS/cysd"));
        let mut ops = Fake { running: vec![(9, cys.join("Contents/MacOS/cysd"))], ..Default::default() };
        let lines = clean_macos(&p, &mut ops);
        assert!(ops.events.is_empty() && cys.exists());
        assert!(p.launch_agents.join(format!("{LEGACY_JOB_LABEL}.plist")).exists());
        assert!(lines[0].contains("실행 중"));
        let _ = fs::remove_dir_all(&t);
    }

    #[cfg(unix)]
    #[test]
    fn terminate_failure_symlink_and_unreadable_roots_are_fail_open() {
        let (t, mut p) = rig("failopen");
        let cys = make_app(&p.app_roots[0], "cys.app", LEGACY_BUNDLE_ID, "cys-app");
        let mut ops = Fake { running: vec![(5, cys.join("Contents/MacOS/cys-app"))], terminate_fail: true, ..Default::default() };
        assert!(clean_macos(&p, &mut ops)[0].contains("사용 중"), "ERRATUM 2: running GUI = in use, never terminated");
        assert!(ops.events.is_empty());
        assert!(cys.exists());
        // a symlink named like the app is never followed
        let target = make_app(&t.join("elsewhere"), "real.app", LEGACY_BUNDLE_ID, "x");
        std::os::unix::fs::symlink(&target, p.app_roots[1].join("link.app")).unwrap();
        p.app_roots.push(t.join("does-not-exist"));
        let mut ops = Fake::default();
        let _ = clean_macos(&p, &mut ops);
        assert!(target.exists() && p.app_roots[1].join("link.app").exists());
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn own_bundle_is_skipped_even_if_it_carried_the_legacy_id() {
        let (t, mut p) = rig("own");
        let cys = make_app(&p.app_roots[0], "cys.app", LEGACY_BUNDLE_ID, "cys-app");
        p.own_bundle = Some(cys.clone());
        clean_macos(&p, &mut Fake::default());
        assert!(cys.exists());
        let _ = fs::remove_dir_all(&t);
    }

    const REG: &str = r#"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Uninstall\cys
    DisplayName    REG_SZ    cys
    Publisher    REG_SZ    Acme Pub
    InstallLocation    REG_SZ    C:\Users\u\AppData\Local\cys
    UninstallString    REG_SZ    "C:\Users\u\AppData\Local\cys\uninstall.exe"

HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Uninstall\Wave Terminal
    DisplayName    REG_SZ    Wave Terminal
    Publisher    REG_SZ    Acme Pub
    InstallLocation    REG_SZ    C:\Users\u\AppData\Local\Wave Terminal
    UninstallString    REG_SZ    x
"#;
    const OWN: &str = r"C:\Users\u\AppData\Local\Wave Terminal";
    fn win_ops() -> Fake {
        Fake { running: vec![(3, PathBuf::from(r"C:\Users\u\AppData\Local\cys\cys-app.exe"))], ..Default::default() }
    }

    /// The real values dumped from the official upstream 0.14.56 install on a Windows runner (quoted paths included).
    const REAL: &str = r#"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Uninstall\cys
    MainBinaryName    REG_SZ    cys-app.exe
    DisplayName    REG_SZ    cys
    DisplayIcon    REG_SZ    "C:\Users\runneradmin\AppData\Local\cys\cys-app.exe"
    DisplayVersion    REG_SZ    0.14.56
    Publisher    REG_SZ    cysjavis
    InstallLocation    REG_SZ    "C:\Users\runneradmin\AppData\Local\cys"
    UninstallString    REG_SZ    "C:\Users\runneradmin\AppData\Local\cys\uninstall.exe"
    NoModify    REG_DWORD    0x1
"#;

    #[test]
    fn real_observed_upstream_entry_matches_and_runs_its_own_uninstaller() {
        let entries = parse_reg_query(REAL);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].publisher, "cysjavis");
        assert_eq!(entries[0].install_location, r"C:\Users\runneradmin\AppData\Local\cys", "quotes stripped");
        let mut ops = Fake { running: vec![(3, PathBuf::from(r"C:\Users\runneradmin\AppData\Local\cys\cys-app.exe"))], ..Default::default() };
        let own = Path::new(r"C:\Users\runneradmin\AppData\Local\Wave Terminal");
        let lines = clean_windows(&entries, own, OBSERVED.as_ref(), &mut ops);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(ops.events[0], "terminate 3");
        assert_eq!(ops.events[1], r#"uninstall "C:\Users\runneradmin\AppData\Local\cys\uninstall.exe""#);
        assert_eq!(uninstaller_path(r#""C:\x y\uninstall.exe""#), PathBuf::from(r"C:\x y\uninstall.exe"));
        // our own install (different subkey/publisher/dir) never matches
        let mut wave = entries[0].clone(); wave.key = wave.key.replace(r"Uninstall\cys", r"Uninstall\Wave Terminal");
        wave.display_name = "Wave Terminal".into(); wave.install_location = own.display().to_string();
        let mut ops = Fake::default();
        assert!(clean_windows(&[wave], own, OBSERVED.as_ref(), &mut ops).is_empty() && ops.events.is_empty());
    }

    #[test]
    fn windows_without_observed_values_is_log_only() {
        let mut ops = win_ops();
        let lines = clean_windows(&parse_reg_query(REG), Path::new(OWN), None, &mut ops);
        assert!(ops.events.is_empty());
        assert!(!lines.is_empty() && lines.iter().all(|l| l.starts_with(LOG_ONLY)), "{lines:?}");
    }

    #[test]
    fn windows_match_needs_every_condition() {
        let obs = Observed { publisher: "Acme Pub" };
        let entries = parse_reg_query(REG);
        let mut ops = win_ops();
        // all conditions hold (cys-app.exe present per fake file_exists) -> terminate GUI then uninstall
        let lines = clean_windows(&entries, Path::new(OWN), Some(&obs), &mut ops);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(ops.events[0], "terminate 3");
        assert!(ops.events[1].starts_with("uninstall "), "{:?}", ops.events);
        // each missing/wrong condition = nothing happens
        let wrong_pub = Observed { publisher: "Someone Else" };
        let mut ops = win_ops();
        assert!(clean_windows(&entries, Path::new(OWN), Some(&wrong_pub), &mut ops).is_empty() && ops.events.is_empty());
        let mut renamed = entries[0].clone(); renamed.key = renamed.key.replace(r"Uninstall\cys", r"Uninstall\cys-notes");
        let mut ops = win_ops();
        assert!(clean_windows(&[renamed], Path::new(OWN), Some(&obs), &mut ops).is_empty() && ops.events.is_empty());
        let mut own = entries[0].clone(); own.install_location = OWN.to_string();
        let mut ops = win_ops();
        assert!(clean_windows(&[own], Path::new(OWN), Some(&obs), &mut ops).is_empty() && ops.events.is_empty());
        let mut ops = win_ops(); ops.no_files = true;
        assert!(clean_windows(&entries, Path::new(OWN), Some(&obs), &mut ops).is_empty() && ops.events.is_empty(), "exe must exist");
    }

    #[test]
    fn cleanup_runs_from_exactly_one_place_the_rename_migration_run() {
        let rm = include_str!("rename_migration.rs");
        let main = include_str!("main.rs");
        assert_eq!(rm.matches("legacy_cleanup::run_at_start(&settings_file)").count(), 1);
        assert_eq!(main.matches("run_at_start(").count(), 0, "no second call site");
        let run_body = &rm[rm.find("pub(crate) async fn run(").unwrap()..];
        assert!(run_body.find("run_at_start").unwrap() < run_body.find("native::run").unwrap(), "before the migration");
    }

    /// C6 rig step (run by hand with RIG=<private /tmp tree>, `--ignored`): the 0.3.6 start sequence on a tree where
    /// the original cys already overwrote the pack. Same functions the startup calls; bootout is a recorded no-op so
    /// the real launchd is never touched.
    struct RigOps { events: Vec<String> }
    impl Ops for RigOps {
        fn processes_under(&self, dir: &Path) -> Vec<(u32, PathBuf)> {
            let mut sys = sysinfo::System::new();
            sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true,
                sysinfo::ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::Always));
            sys.processes().iter().filter_map(|(pid, p)| p.exe().filter(|e| e.starts_with(dir)).map(|e| (pid.as_u32(), e.to_path_buf()))).collect()
        }
        fn terminate(&mut self, pid: u32) -> Result<(), String> { self.events.push(format!("terminate {pid}")); RealOpsTerminate::kill(pid) }
        fn bootout(&mut self, label: &str) -> Result<(), String> { self.events.push(format!("bootout(NO-OP in rig) {label}")); Ok(()) }
        fn run_uninstaller(&mut self, _: &str) -> Result<(), String> { Ok(()) }
        // rig (private /tmp tree, no daemon of its own): read-only process table, no lsof/launchctl ever.
        fn processes(&self) -> Vec<Proc> { RealOps.processes() }
        fn socket_holders(&self, _: &Path) -> Result<Vec<u32>, String> { Ok(vec![]) }
        fn job_running(&self, _: &str) -> Result<bool, String> { Ok(false) }
        fn open_files_under(&self, _: &[u32], _: &Path) -> Result<Vec<u32>, String> { Ok(vec![]) }
    }
    struct RealOpsTerminate;
    impl RealOpsTerminate { fn kill(pid: u32) -> Result<(), String> { RealOps.terminate(pid) } }

    #[test]
    #[ignore]
    fn c6_rig_start_sequence() {
        let tree = PathBuf::from(std::env::var("RIG").expect("RIG tree"));
        assert!(tree.starts_with("/private/tmp"), "rig must be under /private/tmp");
        let paths = Paths { app_roots: vec![tree.join("home/Applications")], launch_agents: tree.join("home/Library/LaunchAgents"),
            archive: tree.join("cfg/removed-cys-apps"), own_bundle: None, stamp: "RIG".into(),
            pack: tree.join("home/.cys/pack"), cys_home: tree.join("home/.cys"), socket: tree.join("home/.local/state/cys/cys.sock"),
            backup_root: tree.join("cfg/pack-backups") };
        let mut ops = RigOps { events: vec![] };
        let lines = clean_macos(&paths, &mut ops);
        println!("CLEAN_LINES {lines:?}\nOPS {:?}", ops.events);
        println!("HEAL {:?}", cys::pack::heal_foreign_pack());
    }

    /// C6 real-shape scan: READ-ONLY on this Mac's real roots. Only `plan_macos` (no mutation) is called.
    #[test]
    #[ignore]
    fn c6_real_shape_scan_readonly() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let paths = Paths { app_roots: vec![PathBuf::from("/Applications"), home.join("Applications")],
            launch_agents: home.join("Library/LaunchAgents"), archive: PathBuf::from("/nonexistent"), own_bundle: None, stamp: "x".into(),
            pack: PathBuf::from("/nonexistent"), cys_home: PathBuf::from("/nonexistent"), socket: PathBuf::from("/nonexistent"),
            backup_root: PathBuf::from("/nonexistent") };
        for l in plan_macos(&paths) { println!("PLAN {l}"); }
        println!("PLAN_DONE");
    }

    /// C7 (Windows runner only, `--ignored`): runs the PRODUCTION entry on the real registry of a disposable runner
    /// where the official upstream cys setup is installed. Compiled on Windows only so it can never touch a Mac.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn c7_windows_real_cleanup() {
        let settings = PathBuf::from(std::env::var("C7_SETTINGS").expect("C7_SETTINGS"));
        if let Some(h) = run_at_start(&settings) { h.join().unwrap(); }   // wait for the background cleanup before the test process exits
        println!("NOTICE {:?}", take_notice());
    }

    #[test]
    fn wait_gone_returns_when_the_marker_disappears_and_false_when_it_stays() {
        let d = std::env::temp_dir().join(format!("wave-wg-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("cys-app.exe");
        std::fs::write(&f, "x").unwrap();
        let f2 = f.clone();
        let h = std::thread::spawn(move || { std::thread::sleep(std::time::Duration::from_millis(1500)); std::fs::remove_file(f2).unwrap(); });
        assert!(wait_gone(&f, 10), "disappearing marker must be reported gone");
        h.join().unwrap();
        std::fs::write(&f, "x").unwrap();
        assert!(!wait_gone(&f, 2), "a marker that stays must not be reported gone");
        assert!(wait_gone(&d.join("never-existed.exe"), 1));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn background_cleanup_never_blocks_the_caller_and_logs_the_truthful_result() {
        let d = std::env::temp_dir().join(format!("wave-bg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let settings = d.join("user-settings.json");
        let t0 = std::time::Instant::now();
        let h = run_in_background(settings.clone(), "진행 중".to_string(), || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            vec!["남은 cys 앱을 제거하지 못해 그대로 둡니다(권한 등): 확인하지 못했습니다".to_string()]
        });
        assert!(t0.elapsed() < std::time::Duration::from_millis(500), "caller blocked for {:?}", t0.elapsed());
        assert_eq!(take_notice().as_deref(), Some("진행 중"), "one-line status visible at once");
        h.join().unwrap();
        let log = std::fs::read_to_string(d.join("legacy-cleanup.log")).unwrap();
        assert!(log.contains("확인하지 못했습니다"), "truthful result is logged: {log}");
        assert!(take_notice().unwrap().contains("확인하지 못했습니다"), "later notice carries the truthful outcome");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn windows_candidates_counts_only_real_matches() {
        let entries = parse_reg_query(REAL);
        let own = Path::new(r"C:\Users\runneradmin\AppData\Local\Wave Terminal");
        let ops = Fake::default();
        assert_eq!(windows_candidates(&entries, own, OBSERVED.as_ref(), &ops), 1);
        assert_eq!(windows_candidates(&entries, own, None, &ops), 0, "disabled = no candidates");
        let mut gone = Fake::default(); gone.no_files = true;
        assert_eq!(windows_candidates(&entries, own, OBSERVED.as_ref(), &gone), 0, "exe missing");
        let mut e2 = entries.clone(); e2[0].publisher = "someone".into();
        assert_eq!(windows_candidates(&e2, own, OBSERVED.as_ref(), &ops), 0);
    }

    // ---------------- ERRATUM 2 (ERRATUM_기준_036_C5_C2_2.md) fixtures ----------------
    fn visible(lines: &[String]) -> Vec<&String> { lines.iter().filter(|l| !l.starts_with(LOG_ONLY)).collect() }

    /// Installs the shipped pack under `p.pack` (thread-local override; returns the guard), then makes it look like the
    /// owner's ops pack: original-cys fingerprint in a probe file, user-edited soul, 0.14.10 version marker.
    fn fingerprinted_pack(t: &Path, p: &Paths) -> cys::pack::DirsOverrideGuard {
        let g = cys::pack::override_dirs_for_thread(&p.pack, t.join("home/.claude"));
        cys::pack::install(false).unwrap();
        fs::write(p.pack.join("bin/javis_preflight.py"), "APP = \"/Applications/cys.app/Contents/MacOS/cys\"\n").unwrap();
        fs::write(p.pack.join("soul.md"), "OWNER EDITED SOUL").unwrap();
        fs::write(p.pack.join(".pack-version"), "0.14.10").unwrap();
        assert!(cys::pack::foreign_pack_drift(&p.pack));
        g
    }
    fn heal() -> Result<Option<String>, String> { cys::pack::heal_foreign_pack() }

    /// THE ops-Mac fixture: fingerprinted custom pack + live seats + our old-fork cys.app (com.cysjavis.terminal 0.14.10)
    /// with its daemon running (holding the shared socket, its launchd job loaded) -> 0 changes + exactly one notice line.
    #[test]
    fn erratum2_ops_mac_fixture_changes_nothing_and_shows_exactly_one_notice() {
        let (t, p) = rig("opsmac");
        let _g = fingerprinted_pack(&t, &p);
        let cys_app = make_app(&p.app_roots[0], "cys.app", LEGACY_BUNDLE_ID, "cys-app");
        let cysd = cys_app.join("Contents/MacOS/cysd");
        make_job(&p.launch_agents, LEGACY_JOB_LABEL, &cysd);
        let _ = sentinels(&t);
        let before = snapshot(&[t.as_path()]);
        let mut ops = Fake {
            procs: vec![
                Proc { pid: 900, exe: Some(cysd.clone()), cwd: Some(PathBuf::from("/")), argv: vec![cysd.display().to_string()] },
                Proc { pid: 901, exe: Some(PathBuf::from("/opt/homebrew/bin/claude")), cwd: Some(t.join("home/.cys/dept-a")), argv: vec!["claude".into()] },
                Proc { pid: 902, exe: Some(PathBuf::from("/usr/local/bin/node")), cwd: Some(t.join("home/work")), argv: vec!["node".into(), "codex".into()] },
            ],
            holders: vec![900], loaded: vec![LEGACY_JOB_LABEL.into()], open_under: vec![900, 901], ..Default::default() };

        let lines = clean_and_heal_macos(&p, &mut ops, &mut heal);

        assert_eq!(snapshot(&[t.as_path()]), before, "0 changes anywhere in the tree (app, plist, pack, records)");
        assert!(ops.events.is_empty(), "no terminate / bootout: {:?}", ops.events);
        assert_eq!(visible(&lines).len(), 1, "exactly one notice line: {lines:?}");
        let _ = fs::remove_dir_all(&t);
    }

    /// Student fixture: no live seat, leftover upstream app (not running, job not loaded) -> app archived, plist removed,
    /// pack healed after a timestamped full backup (user-edited soul + overwritten probe kept, never deleted).
    #[test]
    fn erratum2_student_fixture_cleans_up_backs_up_and_heals() {
        let (t, p) = rig("student");
        let _g = fingerprinted_pack(&t, &p);
        let cys_app = make_app(&p.app_roots[0], "cys.app", LEGACY_BUNDLE_ID, "cys-app");
        make_job(&p.launch_agents, LEGACY_JOB_LABEL, &cys_app.join("Contents/MacOS/cysd"));
        let ds = sentinels(&t);
        let refs: Vec<&Path> = ds.iter().map(|d| d.as_path()).filter(|d| !d.ends_with(".cys")).collect();
        let records = snapshot(&refs);
        let mut ops = Fake { plist: Some(p.launch_agents.join(format!("{LEGACY_JOB_LABEL}.plist"))), ..Default::default() };

        let lines = clean_and_heal_macos(&p, &mut ops, &mut heal);

        assert!(!cys_app.exists() && p.archive.join("T1-cys.app").exists(), "{lines:?}");
        assert!(!p.launch_agents.join(format!("{LEGACY_JOB_LABEL}.plist")).exists());
        assert!(!cys::pack::foreign_pack_drift(&p.pack), "healed: {lines:?}");
        let backup = p.backup_root.join("T1-pack");
        assert_eq!(fs::read_to_string(backup.join("soul.md")).unwrap(), "OWNER EDITED SOUL", "user-edited file backed up");
        assert!(fs::read_to_string(backup.join("bin/javis_preflight.py")).unwrap().contains("/Applications/cys.app"));
        assert_eq!(fs::read_to_string(p.pack.join("soul.md")).unwrap(), "OWNER EDITED SOUL", "user-owned soul untouched");
        assert_eq!(snapshot(&refs), records, "records byte-identical");
        assert_eq!(ops.events, [format!("bootout {LEGACY_JOB_LABEL} plist_present=true")]);
        let _ = fs::remove_dir_all(&t);
    }

    /// C5 gate (ii): app removed in this run but a live seat depends on the pack -> pack untouched + one notice.
    #[test]
    fn erratum2_heal_skipped_when_a_live_seat_depends_on_the_pack() {
        for (tag, ops) in [
            ("cwd", Fake { procs: vec![Proc { pid: 7, exe: Some("/x/claude".into()), cwd: Some(PathBuf::from("CYS/dept")), argv: vec![] }], ..Default::default() }),
            ("argv", Fake { procs: vec![Proc { pid: 7, exe: Some("/x/node".into()), cwd: None, argv: vec!["codex".into(), "--cd".into(), "CYS/pack".into()] }], ..Default::default() }),
            ("open", Fake { procs: vec![Proc { pid: 7, exe: Some("/x/waved".into()), cwd: None, argv: vec![] }], open_under: vec![7], ..Default::default() }),
            ("socket", Fake { holders: vec![55], procs: vec![Proc { pid: 55, exe: Some("/Applications/Wave Terminal.app/Contents/MacOS/waved".into()), cwd: None, argv: vec![] }], ..Default::default() }),
        ] {
            let (t, p) = rig(&format!("seat-{tag}"));
            let _g = fingerprinted_pack(&t, &p);
            let cys_app = make_app(&p.app_roots[0], "cys.app", LEGACY_BUNDLE_ID, "cys-app");
            let pack_before = snapshot(&[p.pack.as_path()]);
            let mut ops = ops;
            let fix = |s: &str| s.replace("CYS", &p.cys_home.display().to_string());
            for pr in &mut ops.procs { pr.cwd = pr.cwd.as_ref().map(|c| PathBuf::from(fix(&c.display().to_string()))); pr.argv = pr.argv.iter().map(|a| fix(a)).collect(); }
            let lines = clean_and_heal_macos(&p, &mut ops, &mut heal);
            assert!(!cys_app.exists(), "{tag}: unused app still removed");
            assert_eq!(snapshot(&[p.pack.as_path()]), pack_before, "{tag}: pack untouched");
            assert!(!p.backup_root.exists(), "{tag}: no backup needed when nothing changes");
            assert!(visible(&lines).iter().any(|l| l.contains("지침")), "{tag}: notice {lines:?}");
            let _ = fs::remove_dir_all(&t);
        }
    }

    /// C5 gate (i): fingerprinted pack but NO leftover app removed in this run -> pack untouched.
    #[test]
    fn erratum2_heal_skipped_without_a_removal_in_the_same_run() {
        let (t, p) = rig("noremoval");
        let _g = fingerprinted_pack(&t, &p);
        let before = snapshot(&[p.pack.as_path()]);
        let lines = clean_and_heal_macos(&p, &mut Fake::default(), &mut heal);
        assert_eq!(snapshot(&[p.pack.as_path()]), before, "{lines:?}");
        let _ = fs::remove_dir_all(&t);
    }

    /// C2 amended: each "old fork in use" signal alone keeps the app (notice only); none = removed.
    #[test]
    fn erratum2_old_fork_in_use_signals_each_keep_the_app() {
        let cases: Vec<(&str, Box<dyn Fn(&Path) -> Fake>, bool)> = vec![
            ("gui-running", Box::new(|a: &Path| Fake { running: vec![(41, a.join("Contents/MacOS/cys-app"))], ..Default::default() }), true),
            ("socket-owner-inside", Box::new(|a: &Path| Fake { procs: vec![Proc { pid: 60, exe: Some(a.join("Contents/MacOS/cysd")), ..Default::default() }], holders: vec![60], ..Default::default() }), true),
            ("socket-owner-unknown", Box::new(|_: &Path| Fake { holders: vec![61], ..Default::default() }), true),
            ("loaded-job-other-label", Box::new(|_: &Path| Fake { loaded: vec!["org.someone.cysd".into()], ..Default::default() }), true),
            ("seat-cwd", Box::new(|a: &Path| Fake { procs: vec![Proc { pid: 70, exe: Some("/x/claude".into()), cwd: Some(a.join("Contents")), argv: vec![] }], ..Default::default() }), true),
            ("seat-argv", Box::new(|a: &Path| Fake { procs: vec![Proc { pid: 71, exe: Some("/x/node".into()), cwd: None, argv: vec!["codex".into(), a.join("Contents/MacOS/cys").display().to_string()] }], ..Default::default() }), true),
            ("job-not-loaded", Box::new(|_: &Path| Fake::default()), false),
        ];
        for (tag, mk, keep) in cases {
            let (t, p) = rig(&format!("inuse-{tag}"));
            let cys_app = make_app(&p.app_roots[0], "cys.app", LEGACY_BUNDLE_ID, "cys-app");
            // a job under ANOTHER label, using `Program` (not ProgramArguments), pointing into the bundle
            fs::create_dir_all(&p.launch_agents).unwrap();
            fs::write(p.launch_agents.join("org.someone.cysd.plist"), format!(r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
<key>Label</key><string>org.someone.cysd</string><key>Program</key><string>{}</string></dict></plist>"#, cys_app.join("Contents/MacOS/cysd").display())).unwrap();
            let mut ops = mk(&cys_app);
            let lines = clean_macos(&p, &mut ops);
            assert_eq!(cys_app.exists(), keep, "{tag}: {lines:?}");
            if keep { assert!(ops.events.is_empty(), "{tag}: nothing stopped {:?}", ops.events); assert_eq!(visible(&lines).len(), 1, "{tag}"); }
            let _ = fs::remove_dir_all(&t);
        }
    }

    /// Every external program the Mac cleanup could run refuses in a test build (fail closed) -> treated as "in use".
    #[cfg(target_os = "macos")]
    #[test]
    fn erratum2_real_external_programs_refuse_in_tests_and_fail_closed() {
        let real = RealOps;
        let sock = std::env::temp_dir().join(format!("wave-guard-{}.sock", std::process::id()));
        fs::write(&sock, "").unwrap();
        assert!(real.socket_holders(&sock).unwrap_err().contains("test guard"));
        assert!(real.job_running("x").unwrap_err().contains("test guard"));
        assert!(real.open_files_under(&[1], Path::new("/x")).unwrap_err().contains("test guard"));
        assert!(RealOps.bootout("x").unwrap_err().contains("test guard"));
        let _ = fs::remove_file(&sock);
    }
}
