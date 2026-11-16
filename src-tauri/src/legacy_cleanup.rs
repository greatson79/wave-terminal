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
}

pub(crate) struct Paths {
    pub app_roots: Vec<PathBuf>,
    pub launch_agents: PathBuf,
    pub archive: PathBuf,
    pub own_bundle: Option<PathBuf>,
    pub stamp: String,
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

pub(crate) fn clean_macos(p: &Paths, ops: &mut dyn Ops) -> Vec<String> {
    let mut lines = Vec::new();
    for (app, name, exe) in find_legacy_apps(p) {
        let main = app.join("Contents/MacOS").join(&exe);
        let procs = ops.processes_under(&app);
        // A daemon from the leftover app may be serving seats: never stop it here (seats stay intact).
        if procs.iter().any(|(_, e)| *e != main) {
            lines.push(format!("남은 cys 앱({name})의 서비스가 실행 중이라 건드리지 않았습니다."));
            continue;
        }
        if let Some((pid, _)) = procs.first() {
            if let Err(e) = ops.terminate(*pid) {
                lines.push(format!("남은 cys 앱({name})을 종료하지 못해 그대로 둡니다: {e}"));
                continue;
            }
        }
        let plist = legacy_plist(p);
        if plist_program(&plist).is_some_and(|prog| prog.starts_with(&app)) {
            let _ = ops.bootout(LEGACY_JOB_LABEL); // not loaded = fine; the file removal below is what matters
            match std::fs::remove_file(&plist) {
                Ok(()) => lines.push(format!("남은 cys 자동 시작 등록({LEGACY_JOB_LABEL})을 정리했습니다.")),
                Err(e) => { lines.push(format!("cys 자동 시작 등록을 지우지 못해 앱도 그대로 둡니다: {e}")); continue; }
            }
        }
        let dest = p.archive.join(format!("{}-{name}", p.stamp));
        let moved = std::fs::create_dir_all(&p.archive).and_then(|_| std::fs::rename(&app, &dest));
        match moved {
            Ok(()) => lines.push(format!("남은 cys 앱({name})을 보관 폴더로 옮겼습니다: {}", dest.display())),
            Err(e) => lines.push(format!("남은 cys 앱({name})을 옮기지 못했습니다(권한 등). 그대로 둡니다: {e}")),
        }
    }
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

/// Lines starting with `LOG_ONLY` go to the log file but not to the user notice.
pub(crate) const LOG_ONLY: &str = "[log-only] ";

/// Windows removal needs ALL of: Uninstall subkey name "cys", DisplayName "cys", Publisher == the observed one,
/// `cys-app.exe` inside InstallLocation, and InstallLocation != our own dir. With `observed == None` it only logs.
pub(crate) fn clean_windows(entries: &[UninstallEntry], own_dir: &Path, observed: Option<&Observed>, ops: &mut dyn Ops) -> Vec<String> {
    let mut lines = Vec::new();
    for e in entries {
        let key_name = e.key.rsplit(['\\', '/']).next().unwrap_or("");
        if !key_name.eq_ignore_ascii_case(LEGACY_DISPLAY_NAME) || e.display_name != LEGACY_DISPLAY_NAME { continue; }
        let Some(obs) = observed else {
            lines.push(format!("{LOG_ONLY}Windows 제거 비활성(원본 설치값 미관측): 후보 {} / {}", e.key, e.install_location));
            continue;
        };
        if e.publisher != obs.publisher || e.install_location.is_empty() || e.uninstall_string.is_empty() { continue; }
        if same_dir(&e.install_location, own_dir) { continue; }
        let dir = PathBuf::from(&e.install_location);
        if !ops.file_exists(&dir.join(LEGACY_EXE)) { continue; }
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
            Ok(()) => lines.push(format!("남은 cys 앱({})을 제거했습니다.", e.install_location)),
            Err(err) => lines.push(format!("남은 cys 앱을 제거하지 못해 그대로 둡니다(권한 등): {err}")),
        }
    }
    lines
}

fn write_log(settings_file: &Path, lines: &[String]) {
    use std::io::Write;
    let Some(dir) = settings_file.parent() else { return };
    let _ = std::fs::create_dir_all(dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("legacy-cleanup.log")) {
        for l in lines { let _ = writeln!(f, "{l}"); }
    }
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
            let out = std::process::Command::new("/bin/launchctl").args(["bootout", &target]).output().map_err(|e| e.to_string())?;
            if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
        }
        #[cfg(not(target_os = "macos"))] { let _ = label; Ok(()) }
    }
    fn run_uninstaller(&mut self, command: &str) -> Result<(), String> {
        #[cfg(windows)] {
            let out = std::process::Command::new(uninstaller_path(command)).arg("/S").output().map_err(|e| e.to_string())?;
            if out.status.success() { Ok(()) } else { Err(format!("종료 코드 {:?}", out.status.code())) }
        }
        #[cfg(not(windows))] { let _ = command; Ok(()) }
    }
}

/// App-start entry (called once, from `rename_migration::run`). Fail-open: never returns an error, never blocks startup.
pub(crate) fn run_at_start(settings_file: &Path) {
    #[allow(unused_mut, unused_assignments)]
    let mut lines: Vec<String> = Vec::new();
    #[cfg(target_os = "macos")] {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        if !home.as_os_str().is_empty() {
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0).to_string();
            let own_bundle = std::env::current_exe().ok().and_then(|e| e.ancestors().find(|a| a.extension().is_some_and(|x| x == "app")).map(Path::to_path_buf));
            let paths = Paths { app_roots: vec![PathBuf::from("/Applications"), home.join("Applications")],
                launch_agents: home.join("Library/LaunchAgents"),
                archive: settings_file.parent().map(|d| d.join("removed-cys-apps")).unwrap_or_else(|| home.join("removed-cys-apps")),
                own_bundle, stamp };
            lines = clean_macos(&paths, &mut RealOps);
        }
    }
    #[cfg(windows)] {
        let mut text = String::new();
        for root in [r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall", r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall"] {
            if let Ok(o) = std::process::Command::new("reg").args(["query", root, "/s"]).output() {
                text.push_str(&String::from_utf8_lossy(&o.stdout)); text.push('\n');
            }
        }
        let own = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)).unwrap_or_default();
        lines = clean_windows(&parse_reg_query(&text), &own, OBSERVED.as_ref(), &mut RealOps);
    }
    let _ = settings_file;
    if lines.is_empty() { return; }
    write_log(settings_file, &lines);
    let shown: Vec<&str> = lines.iter().filter(|l| !l.starts_with(LOG_ONLY)).map(String::as_str).collect();
    if shown.is_empty() { return; }
    if let Ok(mut n) = NOTICE.lock() { *n = Some(shown.join(" · ")); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;

    #[derive(Default)]
    struct Fake { running: Vec<(u32, PathBuf)>, events: Vec<String>, terminate_fail: bool, plist: Option<PathBuf>, no_files: bool }
    impl Ops for Fake {
        fn processes_under(&self, dir: &Path) -> Vec<(u32, PathBuf)> {
            self.running.iter().filter(|(_, e)| e.display().to_string().starts_with(&dir.display().to_string())).cloned().collect()
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
            archive: t.join("cfg/removed-cys-apps"), own_bundle: None, stamp: "T1".into() };
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
        let mut ops = Fake { running: vec![(41, cys.join("Contents/MacOS/cys-app"))], plist: Some(p.launch_agents.join(format!("{LEGACY_JOB_LABEL}.plist"))), ..Default::default() };

        let lines = clean_macos(&p, &mut ops);

        assert_eq!(ops.events, ["terminate 41", format!("bootout {LEGACY_JOB_LABEL} plist_present=true").as_str()]);
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
        assert!(clean_macos(&p, &mut ops)[0].contains("종료하지 못해"));
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
    }
    struct RealOpsTerminate;
    impl RealOpsTerminate { fn kill(pid: u32) -> Result<(), String> { RealOps.terminate(pid) } }

    #[test]
    #[ignore]
    fn c6_rig_start_sequence() {
        let tree = PathBuf::from(std::env::var("RIG").expect("RIG tree"));
        assert!(tree.starts_with("/private/tmp"), "rig must be under /private/tmp");
        let paths = Paths { app_roots: vec![tree.join("home/Applications")], launch_agents: tree.join("home/Library/LaunchAgents"),
            archive: tree.join("cfg/removed-cys-apps"), own_bundle: None, stamp: "RIG".into() };
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
            launch_agents: home.join("Library/LaunchAgents"), archive: PathBuf::from("/nonexistent"), own_bundle: None, stamp: "x".into() };
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
        run_at_start(&settings);
        println!("NOTICE {:?}", take_notice());
    }
}
