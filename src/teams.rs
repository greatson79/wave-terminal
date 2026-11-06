//! Durable learner-owned teams; independent of runtime department registries.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub folder: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub runtime_key: String,
    pub stopped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_engine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_model: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamPatch {
    pub name: Option<String>,
    pub folder: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TeamError {
    pub code: &'static str,
    pub message: String,
}

type Result<T> = std::result::Result<T, TeamError>;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreData {
    schema_version: u32,
    teams: Vec<Team>,
    #[serde(default)]
    starters_offered: bool,
}

pub struct TeamStore {
    root: PathBuf,
}
impl TeamStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn locked(&self) -> Result<std::fs::File> {
        std::fs::create_dir_all(&self.root).map_err(|_| io_error())?;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file = options
            .open(self.root.join("teams.lock"))
            .map_err(|_| io_error())?;
        // Keep the lock inode: OS locks release on exit, even while another process waits.
        file.lock().map_err(|_| {
            error(
                "store_locked",
                "팀 정보를 사용 중입니다. 잠시 후 다시 시도하세요.",
            )
        })?;
        Ok(file)
    }

    fn read(&self) -> Result<StoreData> {
        let path = self.root.join("teams.json");
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let quarantined = std::fs::read_dir(&self.root)
                    .map_err(|_| io_error())?
                    .any(|e| {
                        e.map(|e| {
                            e.file_name()
                                .to_string_lossy()
                                .starts_with("teams.json.corrupt-")
                        })
                        .unwrap_or(true)
                    });
                return if quarantined {
                    Err(corrupt_error())
                } else {
                    Ok(StoreData {
                        schema_version: 1,
                        teams: Vec::new(),
                        starters_offered: false,
                    })
                };
            }
            Err(_) => return Err(io_error()),
        };
        let parsed = (|| {
            let value: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| corrupt_error())?;
            match value.get("schema_version").and_then(|v| v.as_u64()) {
                Some(1) => (),
                Some(_) => return Err(error("unknown_version", "이 팀 정보는 다른 버전에서 저장됐습니다. 원본을 보관했습니다. 앱 버전을 확인하세요.")),
                None => return Err(corrupt_error()),
            }
            let data: StoreData = serde_json::from_value(value).map_err(|_| corrupt_error())?;
            let mut ids = std::collections::HashSet::new();
            for team in &data.teams {
                if !valid_id(&team.id)
                    || !ids.insert(&team.id)
                    || team.runtime_key != format!("team-{}", team.id)
                    || validate_name(&team.name).ok().as_ref() != Some(&team.name)
                    || !Path::new(&team.folder).is_absolute()
                    || chrono::DateTime::parse_from_rfc3339(&team.created_at).is_err()
                    || [&team.color, &team.last_engine, &team.last_model]
                        .iter()
                        .any(|v| v.as_ref().is_some_and(|s| !valid_label(s)))
                {
                    return Err(corrupt_error());
                }
            }
            Ok(data)
        })();
        if parsed.is_err() {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| io_error())?
                .as_nanos();
            let backup = self
                .root
                .join(format!("teams.json.corrupt-{stamp}-{}", new_id()?));
            std::fs::rename(path, backup).map_err(|_| io_error())?;
            if let Ok(dir) = std::fs::File::open(&self.root) {
                let _ = dir.sync_all();
            }
        }
        parsed
    }

    fn write(&self, data: StoreData) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(&data).map_err(|_| io_error())?;
        let temp = self.root.join(format!(".teams.json.{}.tmp", new_id()?));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(|_| io_error())?;
        let result = (|| -> std::io::Result<()> {
            use std::io::Write;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temp, self.root.join("teams.json"))?;
            if let Ok(dir) = std::fs::File::open(&self.root) {
                let _ = dir.sync_all();
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result.map_err(|_| io_error())
    }

    pub fn list(&self) -> Result<Vec<Team>> {
        let _lock = self.locked()?;
        Ok(self.read()?.teams)
    }

    pub fn create(&self, name: String, folder: String) -> Result<Team> {
        let name = validate_name(&name)?;
        validate_folder(&folder)?;
        let _lock = self.locked()?;
        let mut teams = self.read()?;
        let id = new_id()?;
        let team = Team {
            runtime_key: format!("team-{id}"),
            id,
            name,
            folder,
            color: None,
            stopped: true,
            last_engine: None,
            last_model: None,
            created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        };
        teams.teams.push(team.clone());
        self.write(teams)?;
        Ok(team)
    }

    pub fn update(&self, id: String, patch: TeamPatch) -> Result<Team> {
        check_id(&id)?;
        let name = patch.name.as_deref().map(validate_name).transpose()?;
        if let Some(folder) = &patch.folder {
            validate_folder(folder)?;
        }
        if patch.color.as_ref().is_some_and(|s| !valid_label(s)) {
            return Err(error("invalid_color", "팀 색상을 다시 선택하세요."));
        }
        let _lock = self.locked()?;
        let mut teams = self.read()?;
        let team = teams
            .teams
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(not_found)?;
        if let Some(name) = name {
            team.name = name;
        }
        if let Some(folder) = patch.folder {
            team.folder = folder;
        }
        if let Some(color) = patch.color {
            team.color = Some(color);
        }
        let updated = team.clone();
        self.write(teams)?;
        Ok(updated)
    }

    pub fn delete(&self, id: String) -> Result<()> {
        check_id(&id)?;
        let _lock = self.locked()?;
        let mut teams = self.read()?;
        let index = teams
            .teams
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(not_found)?;
        teams.teams.remove(index);
        self.write(teams)
    }
}

fn error(code: &'static str, message: &str) -> TeamError {
    TeamError {
        code,
        message: message.into(),
    }
}
pub fn io_error() -> TeamError {
    error(
        "store_io",
        "팀 정보를 읽거나 저장하지 못했습니다. 저장 공간과 폴더 권한을 확인하세요.",
    )
}
fn corrupt_error() -> TeamError {
    error(
        "corrupt_store",
        "팀 정보를 읽을 수 없어 원본을 보관했습니다. 보관된 팀 정보를 확인하세요.",
    )
}
fn not_found() -> TeamError {
    error(
        "not_found",
        "팀을 찾을 수 없습니다. 팀 목록을 다시 확인하세요.",
    )
}

fn validate_name(name: &str) -> Result<String> {
    use unicode_segmentation::UnicodeSegmentation;
    let trimmed = name.trim();
    let visible = unicode_width::UnicodeWidthStr::width(trimmed) > 0;
    if !visible || trimmed.graphemes(true).count() > 40 || name.chars().any(char::is_control) {
        return Err(error(
            "invalid_name",
            "팀 이름은 제어 문자 없이 1~40자로 입력하세요.",
        ));
    }
    Ok(trimmed.into())
}
pub fn validate_folder(folder: &str) -> Result<()> {
    if !Path::new(folder).is_absolute()
        || !Path::new(folder).is_dir()
        || std::fs::read_dir(folder).is_err()
    {
        return Err(error(
            "invalid_folder",
            "작업 폴더를 찾을 수 없습니다. 폴더를 다시 선택하세요.",
        ));
    }
    Ok(())
}
fn valid_label(value: &str) -> bool {
    value.chars().count() <= 128 && !value.chars().any(char::is_control)
}
fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                *c == b'-'
            } else {
                c.is_ascii_digit() || (b'a'..=b'f').contains(c)
            }
        })
        && bytes[14] == b'4'
        && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
}
fn check_id(id: &str) -> Result<()> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(error(
            "invalid_id",
            "팀 정보가 올바르지 않습니다. 팀을 다시 선택하세요.",
        ))
    }
}
fn new_id() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|_| io_error())?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct CorruptStore {
    pub quarantined_path: String,
    pub message_ko: String,
    pub message_en: String,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct StoreStatus {
    pub schema_version: u32,
    pub starters_offered: bool,
    pub corrupt: Option<CorruptStore>,
}

pub fn is_team_change_event(event: &serde_json::Value) -> bool {
    event["type"] == "event" && event["name"] == "team.changed"
}

pub fn learner_store() -> TeamStore {
    TeamStore::new(crate::home_dir().join(".cys"))
}

impl TeamStore {
    pub fn mark_starters_offered(&self) -> Result<()> {
        let _lock = self.locked()?;
        let mut data = self.read()?;
        if data.starters_offered {
            return Ok(());
        }
        data.starters_offered = true;
        self.write(data)
    }

    pub fn status(&self) -> Result<StoreStatus> {
        let _lock = self.locked()?;
        match self.read() {
            Ok(data) => Ok(StoreStatus {
                schema_version: 1,
                starters_offered: data.starters_offered,
                corrupt: None,
            }),
            Err(e) if matches!(e.code, "corrupt_store" | "unknown_version") => {
                let mut backups = std::fs::read_dir(&self.root)
                    .map_err(|_| io_error())?
                    .collect::<std::io::Result<Vec<_>>>()
                    .map_err(|_| io_error())?;
                backups.retain(|p| {
                    p.file_name()
                        .to_string_lossy()
                        .starts_with("teams.json.corrupt-")
                });
                backups.sort_by_key(|p| p.file_name());
                let path = backups.last().ok_or_else(io_error)?.path();
                let bytes = std::fs::read(&path).map_err(|_| io_error())?;
                let unknown = serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .and_then(|v| v.get("schema_version").and_then(|v| v.as_u64()))
                    .is_some_and(|v| v != 1);
                Ok(StoreStatus {
                    schema_version: 1,
                    starters_offered: false,
                    corrupt: Some(CorruptStore {
                        quarantined_path: path.to_string_lossy().into_owned(),
                        message_ko: if unknown {
                            "다른 버전의 팀 정보를 보관했습니다. 앱 버전을 확인하세요."
                        } else {
                            "팀 정보를 읽을 수 없어 원본을 보관했습니다. 보관된 파일을 확인하세요."
                        }
                        .into(),
                        message_en: if unknown {
                            "Unsupported team store version; original preserved for recovery."
                        } else {
                            "Invalid team store; original preserved for recovery."
                        }
                        .into(),
                    }),
                })
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture {
        home: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let home = std::env::temp_dir().join(format!(
                "wave-teams-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(home.join(".cys")).unwrap();
            fs::create_dir_all(home.join("work 한글")).unwrap();
            Self { home }
        }
        fn store(&self) -> TeamStore {
            TeamStore::new(self.home.join(".cys"))
        }
        fn folder(&self) -> String {
            self.home.join("work 한글").to_str().unwrap().into()
        }
        fn file(&self) -> PathBuf {
            self.home.join(".cys/teams.json")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.home).unwrap();
        }
    }

    #[test]
    fn teams_atomic_readers_never_observe_partial_json_and_ignore_abandoned_temp() {
        let f = Fixture::new();
        let team = f.store().create("기획팀".into(), f.folder()).unwrap();
        let before = fs::read(f.file()).unwrap();
        fs::write(f.home.join(".cys/.teams.json.interrupted.tmp"), b"{partial").unwrap();
        assert_eq!(f.store().list().unwrap(), vec![team.clone()]);
        assert_eq!(fs::read(f.file()).unwrap(), before);
        let path = f.file();
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done_read = done.clone();
        let reader = std::thread::spawn(move || {
            let mut reads = 0;
            while !done_read.load(std::sync::atomic::Ordering::Acquire) {
                let value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                assert_eq!(value["schema_version"], 1);
                assert_eq!(value["teams"].as_array().unwrap().len(), 1);
                reads += 1;
            }
            reads
        });
        for i in 0..40 {
            f.store()
                .update(
                    team.id.clone(),
                    TeamPatch {
                        name: Some(format!("기획팀 {i}")),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        done.store(true, std::sync::atomic::Ordering::Release);
        assert!(reader.join().unwrap() > 0);
    }

    #[test]
    fn teams_failed_atomic_replace_keeps_target_and_cleans_temp() {
        let f = Fixture::new();
        fs::create_dir(f.file()).unwrap();
        let sentinel = f.file().join("keep");
        fs::write(&sentinel, b"original").unwrap();
        let _lock = f.store().locked().unwrap();
        assert_eq!(
            f.store()
                .write(StoreData {
                    schema_version: 1,
                    teams: Vec::new(),
                    starters_offered: false
                })
                .unwrap_err()
                .code,
            "store_io"
        );
        assert_eq!(fs::read(sentinel).unwrap(), b"original");
        assert!(!fs::read_dir(f.home.join(".cys"))
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().ends_with(".tmp")));
    }

    #[test]
    fn teams_decomposed_korean_counts_visible_graphemes() {
        let f = Fixture::new();
        assert!(f.store().create("가".repeat(40), f.folder()).is_ok());
        assert_eq!(
            f.store()
                .create("가".repeat(41), f.folder())
                .unwrap_err()
                .code,
            "invalid_name"
        );
        assert!(f.store().create("👩‍💻".repeat(40), f.folder()).is_ok());
    }

    #[test]
    fn teams_store_rejects_duplicate_or_mutated_persisted_identity() {
        for mutation in ["duplicate", "runtime", "id", "name", "folder", "time"] {
            let f = Fixture::new();
            f.store().create("기획팀".into(), f.folder()).unwrap();
            let mut root: serde_json::Value =
                serde_json::from_slice(&fs::read(f.file()).unwrap()).unwrap();
            match mutation {
                "duplicate" => {
                    let copy = root["teams"][0].clone();
                    root["teams"].as_array_mut().unwrap().push(copy);
                }
                "runtime" => root["teams"][0]["runtime_key"] = "team-foreign".into(),
                "id" => root["teams"][0]["id"] = "../../outside".into(),
                "name" => root["teams"][0]["name"] = "".into(),
                "folder" => root["teams"][0]["folder"] = "relative".into(),
                "time" => root["teams"][0]["created_at"] = "bad".into(),
                _ => unreachable!(),
            }
            fs::write(f.file(), serde_json::to_vec(&root).unwrap()).unwrap();
            assert_eq!(f.store().list().unwrap_err().code, "corrupt_store");
        }
    }

    #[test]
    fn teams_process_writer() {
        let Some(root) = std::env::var_os("CA_TEAMS_TEST_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        assert!(root.starts_with(std::env::temp_dir()));
        fs::write(root.join(format!("ready-{}", std::process::id())), b"ready").unwrap();
        TeamStore::new(root.join(".cys"))
            .create(
                "기획팀".into(),
                root.join("work 한글").to_str().unwrap().into(),
            )
            .unwrap();
    }

    #[test]
    fn teams_os_lock_serializes_independent_processes() {
        let f = Fixture::new();
        let lock = f.store().locked().unwrap();
        let mut children: Vec<_> = (0..4)
            .map(|_| {
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "teams::tests::teams_process_writer",
                        "--nocapture",
                    ])
                    .env("CA_TEAMS_TEST_ROOT", &f.home)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .spawn()
                    .unwrap()
            })
            .collect();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while children
            .iter()
            .any(|c| !f.home.join(format!("ready-{}", c.id())).exists())
        {
            assert!(
                std::time::Instant::now() < deadline,
                "child did not reach lock"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        for child in &mut children {
            assert!(
                child.try_wait().unwrap().is_none(),
                "child bypassed OS lock"
            );
        }
        drop(lock);
        for mut child in children {
            assert!(child.wait().unwrap().success());
        }
        assert_eq!(f.store().list().unwrap().len(), 4);
    }

    #[cfg(unix)]
    #[test]
    fn teams_reaper_and_down_sock_preserve_saved_definitions_with_stub_runtime() {
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new();
        let team = f.store().create("기획팀".into(), f.folder()).unwrap();
        let before = fs::read(f.file()).unwrap();
        let bin = f.home.join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        for name in ["cys", "cysd"] {
            let path = bin.join(name);
            fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let pack = f.home.join("pack");
        fs::create_dir_all(pack.join("bin")).unwrap();
        fs::write(
            pack.join("bin/javis_phoenix.py"),
            "import sys\nsys.exit(0)\n",
        )
        .unwrap();
        let socket = f
            .home
            .join(format!("cys-dept-{}/cys.sock", team.runtime_key));
        let registry = f.home.join(".cys/depts.json");
        let dept = Path::new(env!("CARGO_MANIFEST_DIR")).join("cysjavis-pack/bin/cys-dept");
        for verb in ["reap", "down-sock"] {
            fs::write(&registry, serde_json::to_vec(&serde_json::json!({"depts": {team.runtime_key.clone(): {"socket": socket, "pack_dir": pack}}})).unwrap()).unwrap();
            let mut command = std::process::Command::new("bash");
            command.arg(&dept).arg(verb);
            if verb == "down-sock" {
                command.arg(&socket);
            }
            let output = command
                .env_clear()
                .env("HOME", &f.home)
                .env(
                    "PATH",
                    format!(
                        "{}:/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin",
                        bin.display()
                    ),
                )
                .env("CYS_DEPTS_JSON", &registry)
                .env("CYS_PACK_DIR", &pack)
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{verb}: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(fs::read(f.file()).unwrap(), before);
            assert_eq!(f.store().list().unwrap(), vec![team.clone()]);
            let depts: serde_json::Value =
                serde_json::from_slice(&fs::read(&registry).unwrap()).unwrap();
            assert_eq!(
                depts["depts"],
                serde_json::json!({}),
                "fixture must actually remove runtime entry"
            );
        }
    }

    #[test]
    fn teams_starter_offer_is_explicit_persistent_idempotent_and_never_seeds() {
        let f = Fixture::new();
        let before = f.store().status().unwrap();
        assert!(!before.starters_offered && before.corrupt.is_none());
        assert!(!f.file().exists());
        f.store().mark_starters_offered().unwrap();
        let saved = fs::read(f.file()).unwrap();
        f.store().mark_starters_offered().unwrap();
        assert_eq!(fs::read(f.file()).unwrap(), saved);
        assert!(f.store().status().unwrap().starters_offered);
        let team = f.store().create("기획팀".into(), f.folder()).unwrap();
        f.store()
            .update(
                team.id.clone(),
                TeamPatch {
                    name: Some("변경".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(f.store().status().unwrap().starters_offered);
        f.store().delete(team.id).unwrap();
        assert!(f.store().status().unwrap().starters_offered);
        assert!(f.store().list().unwrap().is_empty());
        // Existing schema-v1 stores without the additive field remain valid and are not rewritten on read.
        fs::write(f.file(), br#"{"schema_version":1,"teams":[]}"#).unwrap();
        assert!(!f.store().status().unwrap().starters_offered);
        assert_eq!(
            fs::read(f.file()).unwrap(),
            br#"{"schema_version":1,"teams":[]}"#
        );
    }

    #[test]
    fn teams_status_reports_quarantined_bytes_across_restart() {
        for bytes in [
            b"{bad".as_slice(),
            br#"{"schema_version":2,"teams":[],"starters_offered":true}"#,
        ] {
            let f = Fixture::new();
            fs::write(f.file(), bytes).unwrap();
            let first = f.store().status().unwrap();
            assert!(!first.starters_offered);
            let corrupt = first.corrupt.as_ref().unwrap();
            assert_eq!(fs::read(&corrupt.quarantined_path).unwrap(), bytes);
            assert!(!corrupt.message_ko.is_ascii());
            assert!(corrupt.message_en.is_ascii());
            assert_eq!(f.store().status().unwrap(), first);
            assert!(f.store().mark_starters_offered().is_err());
            assert!(!f.file().exists());
        }
    }

    #[test]
    fn teams_crud_duplicate_names_identity_and_restart() {
        let f = Fixture::new();
        assert!(f.store().list().unwrap().is_empty());
        assert!(!f.file().exists(), "listing must not seed or migrate");
        let a = f
            .store()
            .create("  기획 · 팀  ".into(), f.folder())
            .unwrap();
        let b = f.store().create(a.name.clone(), f.folder()).unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(a.name, "기획 · 팀");
        assert_eq!(a.runtime_key, format!("team-{}", a.id));
        assert_eq!(a.id.len(), 36);
        assert_eq!(&a.id[14..15], "4");
        assert!(matches!(a.id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        assert!(a.stopped);
        assert!(a.last_engine.is_none() && a.last_model.is_none());
        assert_eq!(f.store().list().unwrap(), vec![a.clone(), b.clone()]);
        let renamed = f
            .store()
            .update(
                a.id.clone(),
                TeamPatch {
                    name: Some("제작팀".into()),
                    color: Some("#123abc".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(renamed.id, a.id);
        assert_eq!(renamed.runtime_key, a.runtime_key);
        assert_eq!(renamed.created_at, a.created_at);
        assert_eq!(renamed.name, "제작팀");
        f.store().delete(a.id).unwrap();
        f.store().delete(b.id).unwrap();
        assert!(
            f.store().list().unwrap().is_empty(),
            "deleted starters must never return"
        );
        let json: serde_json::Value = serde_json::from_slice(&fs::read(f.file()).unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"schema_version":1,"teams":[],"starters_offered":false})
        );
    }

    #[test]
    fn teams_validation_and_failed_mutation_preserve_bytes() {
        let f = Fixture::new();
        let a = f.store().create("검토팀".into(), f.folder()).unwrap();
        let before = fs::read(f.file()).unwrap();
        for name in [
            "".into(),
            "   ".into(),
            "a\nb".into(),
            "\t팀".into(),
            "가".repeat(41),
            "\u{200b}".into(),
            "\u{2060}".into(),
            "\u{200e}".into(),
        ] {
            assert_eq!(
                f.store().create(name, f.folder()).unwrap_err().code,
                "invalid_name"
            );
        }
        assert!(f.store().create("가".repeat(40), f.folder()).is_ok());
        // Restore the fixture to inspect rejected updates against the same persisted value.
        fs::write(f.file(), &before).unwrap();
        for folder in [
            "relative".to_string(),
            f.home.join("missing").display().to_string(),
            f.file().display().to_string(),
        ] {
            let e = f
                .store()
                .update(
                    a.id.clone(),
                    TeamPatch {
                        folder: Some(folder),
                        ..Default::default()
                    },
                )
                .unwrap_err();
            assert_eq!(e.code, "invalid_folder");
            assert_eq!(
                e.message,
                "작업 폴더를 찾을 수 없습니다. 폴더를 다시 선택하세요."
            );
            assert_eq!(fs::read(f.file()).unwrap(), before);
        }
        assert_eq!(
            f.store().delete("../../other".into()).unwrap_err().code,
            "invalid_id"
        );
        assert_eq!(
            f.store()
                .delete("00000000-0000-4000-8000-000000000000".into())
                .unwrap_err()
                .code,
            "not_found"
        );
        assert!(serde_json::from_str::<TeamPatch>(r#"{"id":"replacement"}"#).is_err());
        assert_eq!(fs::read(f.file()).unwrap(), before);
    }

    #[test]
    fn teams_missing_saved_folder_does_not_hide_definitions() {
        let f = Fixture::new();
        let a = f.store().create("기획팀".into(), f.folder()).unwrap();
        fs::remove_dir(f.folder()).unwrap();
        assert_eq!(f.store().list().unwrap(), vec![a.clone()]);
        assert!(f
            .store()
            .update(
                a.id.clone(),
                TeamPatch {
                    name: Some("새 이름".into()),
                    ..Default::default()
                }
            )
            .is_ok());
        f.store().delete(a.id).unwrap();
    }

    #[test]
    fn teams_corrupt_and_unknown_version_quarantine_and_stay_visible() {
        for bytes in [
            b"{broken".as_slice(),
            br#"{"schema_version":2,"teams":[]}"#,
            br#"{"schema_version":1,"teams":[{}]}"#,
        ] {
            let f = Fixture::new();
            fs::write(f.file(), bytes).unwrap();
            let e = f.store().list().unwrap_err();
            assert!(matches!(e.code, "corrupt_store" | "unknown_version"));
            assert!(!e.message.is_ascii());
            assert!(!f.file().exists());
            let backups: Vec<_> = fs::read_dir(f.file().parent().unwrap())
                .unwrap()
                .flatten()
                .filter(|p| {
                    p.file_name()
                        .to_string_lossy()
                        .starts_with("teams.json.corrupt-")
                })
                .collect();
            assert_eq!(backups.len(), 1);
            assert_eq!(fs::read(backups[0].path()).unwrap(), bytes);
            assert!(
                f.store().list().is_err(),
                "restart must retain visible error"
            );
            assert!(
                f.store().create("기획팀".into(), f.folder()).is_err(),
                "never overwrite with starters"
            );
            assert!(!f.file().exists());
        }
    }

    #[test]
    fn teams_concurrent_writers_do_not_lose_updates() {
        let f = Fixture::new();
        let threads: Vec<_> = (0..12)
            .map(|_| {
                let root = f.home.join(".cys");
                let folder = f.folder();
                std::thread::spawn(move || {
                    TeamStore::new(root)
                        .create("같은 이름".into(), folder)
                        .unwrap()
                })
            })
            .collect();
        let mut ids: Vec<_> = threads.into_iter().map(|t| t.join().unwrap().id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 12);
        assert_eq!(f.store().list().unwrap().len(), 12);
        assert!(!fs::read_dir(f.file().parent().unwrap())
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().contains(".tmp")));
    }

    #[test]
    fn teams_create_and_delete_only_touch_metadata() {
        let f = Fixture::new();
        let depts = f.home.join(".cys/depts.json");
        fs::write(&depts, b"legacy bytes").unwrap();
        let project = PathBuf::from(f.folder()).join("keep.txt");
        fs::write(&project, b"learner files").unwrap();
        let a = f.store().create("기획팀".into(), f.folder()).unwrap();
        f.store().delete(a.id).unwrap();
        assert_eq!(fs::read(depts).unwrap(), b"legacy bytes");
        assert_eq!(fs::read(project).unwrap(), b"learner files");
        let mut files: Vec<_> = fs::read_dir(f.home.join(".cys"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        assert_eq!(files, vec!["depts.json", "teams.json", "teams.lock"]);
    }
}
