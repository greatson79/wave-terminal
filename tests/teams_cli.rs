//! Real CLI subprocesses use temporary HOME and never start or contact the host daemon.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[path = "../src/bin/cysd/team_events.rs"]
mod daemon_team_events;
#[path = "../src/bin/cysd/events.rs"]
mod events;
#[path = "../src-tauri/src/team_events.rs"]
mod team_events;

struct Fixture {
    home: PathBuf,
    socket: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        #[cfg(windows)]
        assert!(cfg!(feature = "test-home-seam"), "Windows CLI tests require the test-home-seam feature; HOME/USERPROFILE do not isolate the OS profile");
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!(
            "wave-team-cli 최형석 {}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(home.join("내 팀 폴더")).unwrap();
        #[cfg(unix)]
        let socket = std::env::temp_dir().join(format!("wt-{}-{unique}.sock", std::process::id()));
        #[cfg(windows)]
        let socket = PathBuf::from(format!(
            r"\\.\pipe\wave-team-cli 최형석 {}-{unique}",
            std::process::id()
        ));
        let fixture = Self { home, socket };
        #[cfg(windows)]
        {
            let identity = fixture.json(&["phoenix-identity"]);
            assert_eq!(
                identity["home_dir"].as_str(),
                fixture.home.to_str(),
                "Windows test binary did not isolate CYS_HOME; refusing team mutations"
            );
        }
        fixture
    }
    fn folder(&self) -> String {
        self.home.join("내 팀 폴더").to_str().unwrap().into()
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cys"));
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("CYS_HOME", &self.home)
            .env("CYS_PACK_DIR", self.home.join("pack"))
            .env("CYS_SOCKET", &self.socket)
            .env("CYS_NO_AUTOSTART", "1")
            .stdin(Stdio::null());
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }
    fn json(&self, args: &[&str]) -> Value {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "unexpected stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.home).unwrap();
        #[cfg(unix)]
        if self.socket.exists() {
            std::fs::remove_file(&self.socket).unwrap();
        }
    }
}

struct FakeTransport {
    runtime: tokio::runtime::Runtime,
    #[cfg(unix)]
    listener: tokio::net::UnixListener,
    #[cfg(windows)]
    listener: tokio::net::windows::named_pipe::NamedPipeServer,
}
impl FakeTransport {
    fn bind(path: &Path) -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let listener = {
            let _entered = runtime.enter();
            #[cfg(unix)]
            let listener = tokio::net::UnixListener::bind(path).unwrap();
            #[cfg(windows)]
            let listener = tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(true)
                .create(path)
                .unwrap();
            listener
        };
        Self { runtime, listener }
    }

    fn receive(self) -> (Value, Value) {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        self.runtime.block_on(async move {
            #[cfg(unix)]
            let (mut stream, _) =
                tokio::time::timeout(std::time::Duration::from_secs(5), self.listener.accept())
                    .await
                    .expect("CLI did not publish after mutation")
                    .unwrap();
            #[cfg(windows)]
            let mut stream = {
                tokio::time::timeout(std::time::Duration::from_secs(5), self.listener.connect())
                    .await
                    .expect("CLI did not publish after mutation")
                    .unwrap();
                self.listener
            };
            let mut line = String::new();
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                BufReader::new(&mut stream).read_line(&mut line),
            )
            .await
            .expect("CLI did not send a complete request")
            .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], "team.changed");
            let bus = events::EventBus::new(None);
            let mut subscriber = bus.subscribe();
            daemon_team_events::publish(&bus);
            let event = subscriber.try_recv().unwrap();
            assert_eq!(bus.tail(1), vec![event.clone()]);
            // Preserve the real event-stream serialization boundary before the app forwarder.
            let received_event =
                serde_json::from_slice(&serde_json::to_vec(&event).unwrap()).unwrap();
            stream
                .write_all(b"{\"id\":1,\"ok\":true,\"result\":null}\n")
                .await
                .unwrap();
            (request, received_event)
        })
    }

    fn assert_no_connection(&self) {
        self.runtime.block_on(async {
            #[cfg(unix)]
            let connected = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                self.listener.accept(),
            )
            .await;
            #[cfg(windows)]
            let connected = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                self.listener.connect(),
            )
            .await;
            assert!(
                connected.is_err(),
                "read or rejected mutation contacted the event endpoint"
            );
        });
    }
}

#[test]
fn teams_cli_real_create_list_update_delete_and_restart_use_shared_store() {
    let f = Fixture::new();
    assert_eq!(f.json(&["team", "list", "--json"]), json!([]));
    assert!(
        !f.home.join(".cys/teams.json").exists(),
        "read never seeds metadata"
    );
    let a = f.json(&[
        "team",
        "create",
        "--name",
        "기획 · 팀",
        "--folder",
        &f.folder(),
        "--json",
    ]);
    let id = a["id"].as_str().unwrap();
    assert_eq!(a["name"], "기획 · 팀");
    assert_eq!(a["folder"], f.folder());
    assert_eq!(a["runtime_key"], format!("team-{id}"));
    assert_eq!(a["stopped"], true);
    assert_eq!(f.json(&["team", "list", "--json"]), json!([a.clone()]));
    let from_core = cys::teams::TeamStore::new(f.home.join(".cys"))
        .list()
        .unwrap();
    assert_eq!(serde_json::to_value(from_core).unwrap(), json!([a.clone()]));
    let renamed = f.json(&["team", "update", "--id", id, "--name", "제작팀", "--json"]);
    assert_eq!(renamed["id"], id);
    assert_eq!(renamed["runtime_key"], a["runtime_key"]);
    assert_eq!(renamed["name"], "제작팀");
    assert_eq!(
        f.json(&["team", "delete", "--id", id, "--json"]),
        Value::Null
    );
    assert_eq!(f.json(&["team", "list", "--json"]), json!([]));
    assert!(!f.home.join(".cys/depts.json").exists());
    assert!(!f.home.join("pack").exists());
    assert_eq!(
        std::fs::read_dir(f.home.join("내 팀 폴더"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn teams_cli_real_create_prints_id_and_validation_errors_are_bilingual_with_exit_two() {
    let f = Fixture::new();
    let output = f.run(&[
        "team",
        "create",
        "--name",
        "검토팀",
        "--folder",
        &f.folder(),
    ]);
    assert!(output.status.success());
    let id = String::from_utf8(output.stdout).unwrap();
    assert_eq!(id.trim().len(), 36);
    let path = f.home.join(".cys/teams.json");
    let before = std::fs::read(&path).unwrap();
    for args in [
        vec!["team", "create", "--name", "", "--folder", &f.folder()],
        vec!["team", "create", "--name", "기획팀", "--folder", "relative"],
    ] {
        let output = f.run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert_eq!(error.lines().count(), 1);
        assert!(error.contains(" / "));
        assert!(error.contains("팀") || error.contains("폴더"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    let missing = f.run(&["team", "create", "--name", "팀"]);
    assert_eq!(missing.status.code(), Some(2));
    let error = String::from_utf8(missing.stderr).unwrap();
    assert!(error.contains("팀 명령") && error.contains("Check the team"));
}

#[test]
fn teams_cli_status_and_unknown_version_keep_original_and_use_exit_one() {
    let f = Fixture::new();
    let status = f.json(&["team", "status", "--json"]);
    assert_eq!(status["schema_version"], 1);
    assert_eq!(status["starters_offered"], false);
    assert!(status["corrupt"].is_null());
    std::fs::write(
        f.home.join(".cys/teams.json"),
        br#"{"schema_version":99,"teams":[]}"#,
    )
    .unwrap();
    let output = f.run(&["team", "list", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8(output.stderr).unwrap();
    assert_eq!(error.lines().count(), 1);
    assert!(error.contains("unsupported version"));
    let status = f.json(&["team", "status", "--json"]);
    let quarantined = Path::new(status["corrupt"]["quarantined_path"].as_str().unwrap());
    assert!(quarantined.starts_with(&f.home));
    assert_eq!(
        std::fs::read(quarantined).unwrap(),
        br#"{"schema_version":99,"teams":[]}"#
    );
    let output = f.run(&[
        "team",
        "create",
        "--name",
        "기획팀",
        "--folder",
        &f.folder(),
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!f.home.join(".cys/teams.json").exists());
}

#[test]
fn teams_cli_write_notifies_existing_socket_and_event_is_forwardable() {
    let f = Fixture::new();
    let listener = FakeTransport::bind(&f.socket);
    let receiver = std::thread::spawn(move || listener.receive());
    f.json(&[
        "team",
        "create",
        "--name",
        "기획팀",
        "--folder",
        &f.folder(),
        "--json",
    ]);
    let (request, event) = receiver.join().unwrap();
    assert_eq!(request["method"], "team.changed");
    assert_eq!(request["params"], json!({}));
    assert_eq!(event["type"], "event");
    assert_eq!(event["name"], "team.changed");
    let mut forwarded = Vec::new();
    team_events::forward(&event, |name, payload| {
        forwarded.push((name.to_string(), payload))
    });
    assert_eq!(forwarded, vec![("teams-changed".to_string(), json!({}))]);
    team_events::forward(&json!({"name":"surface.closed"}), |name, payload| {
        forwarded.push((name.to_string(), payload))
    });
    assert_eq!(forwarded.len(), 1);
}

#[test]
fn teams_cli_reads_and_rejected_writes_do_not_notify() {
    let f = Fixture::new();
    let listener = FakeTransport::bind(&f.socket);
    assert_eq!(f.json(&["team", "list", "--json"]), json!([]));
    assert_eq!(
        f.run(&["team", "create", "--name", "", "--folder", &f.folder()])
            .status
            .code(),
        Some(2)
    );
    listener.assert_no_connection();
}
