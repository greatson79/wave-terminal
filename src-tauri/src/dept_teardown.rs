use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};
use tokio::time::Instant;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    pid: u32,
    started: u64,
}
type Snapshot = HashMap<u32, (Option<u32>, u64)>;

pub struct ProcessTree {
    root: Identity,
    identities: Vec<Identity>,
}
impl ProcessTree {
    pub fn capture(pids: &[u32]) -> Result<Self, String> {
        if pids.is_empty()
            || pids
                .iter()
                .any(|pid| *pid <= 1 || *pid == std::process::id())
        {
            return Err(
                "부서 프로세스 정보를 확인하지 못했습니다. 잠시 후 다시 시도하세요.".into(),
            );
        }
        let snapshot = snapshot()?;
        if !snapshot.contains_key(&pids[0]) {
            return Err(
                "부서 프로세스의 신원을 확인하지 못했습니다. 잠시 후 다시 시도하세요.".into(),
            );
        }
        Ok(Self {
            root: Identity {
                pid: pids[0],
                started: snapshot[&pids[0]].1,
            },
            identities: descendants(pids, &snapshot)?,
        })
    }

    fn remaining(&mut self, snapshot: &Snapshot) -> Result<bool, String> {
        // Retain dead-parent links: Windows preserves PPID after the parent exits.
        // A reused parent PID is excluded; this is observation, not containment.
        let mut seeds = Vec::new();
        for id in &self.identities {
            match snapshot.get(&id.pid) {
                Some((_, 0)) => {
                    return Err(
                        "프로세스 시작 시각을 읽지 못해 부서 종료를 확인할 수 없습니다.".into(),
                    )
                }
                Some((_, started)) if *started != id.started => (),
                _ => seeds.push(id.pid),
            }
        }
        let earliest = self
            .identities
            .iter()
            .map(|id| id.started)
            .min()
            .unwrap_or(0);
        for id in descendants(&seeds, snapshot)? {
            if id.started >= earliest && !self.identities.contains(&id) {
                self.identities.push(id);
            }
        }
        Ok(self
            .identities
            .iter()
            .any(|id| snapshot.get(&id.pid).is_some_and(|p| p.1 == id.started)))
    }
}

pub fn process_ids(identity: &Value, surfaces: &Value) -> Result<Vec<u32>, String> {
    fn pid(value: &Value) -> Result<u32, String> {
        value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n > 1)
            .ok_or_else(|| {
                "부서 프로세스 정보를 확인하지 못했습니다. 잠시 후 다시 시도하세요.".into()
            })
    }
    let mut ids = vec![pid(&identity["daemon_pid"])?];
    for surface in surfaces["surfaces"].as_array().ok_or_else(|| {
        "부서 직원 목록을 확인하지 못했습니다. 잠시 후 다시 시도하세요.".to_string()
    })? {
        if surface["exited"] != true {
            let id = pid(&surface["pid"])?;
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    Ok(ids)
}

fn snapshot() -> Result<Snapshot, String> {
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    snapshot_from_system(&system)
}

fn snapshot_from_system(system: &System) -> Result<Snapshot, String> {
    if system
        .process(sysinfo::Pid::from_u32(std::process::id()))
        .is_none()
    {
        return Err("프로세스 목록을 읽지 못해 부서 종료를 확인할 수 없습니다.".into());
    }
    Ok(system
        .processes()
        .iter()
        .filter(|(_, p)| !matches!(p.status(), ProcessStatus::Zombie | ProcessStatus::Dead))
        .map(|(pid, p)| {
            (
                pid.as_u32(),
                (p.parent().map(|p| p.as_u32()), p.start_time()),
            )
        })
        .collect())
}

fn descendants(pids: &[u32], snapshot: &Snapshot) -> Result<Vec<Identity>, String> {
    let mut ids: HashSet<u32> = pids.iter().copied().collect();
    loop {
        let before = ids.len();
        for (pid, (parent, _)) in snapshot {
            if parent.is_some_and(|p| ids.contains(&p)) {
                ids.insert(*pid);
            }
        }
        if ids.len() == before {
            break;
        }
    }
    ids.into_iter()
        .filter_map(|pid| snapshot.get(&pid).map(|(_, started)| (pid, *started)))
        .map(|(pid, started)| {
            if started == 0 {
                Err("프로세스 시작 시각을 읽지 못해 부서 종료를 확인할 수 없습니다.".into())
            } else {
                Ok(Identity { pid, started })
            }
        })
        .collect()
}

const RECEIPT_PREFIX: &str = "CYS_DEPT_STOP_RESULT ";

fn verify_receipt(stdout: &[u8], root: &Identity) -> Result<(), String> {
    let invalid = || {
        "부서 종료 확인 영수증이 없거나 올바르지 않습니다. 기존 파일은 유지됩니다. cys-dept.new 후보와 기존 업데이트 안내를 확인한 뒤 다시 시도하세요.".to_string()
    };
    let text = std::str::from_utf8(stdout).map_err(|_| invalid())?;
    let mut lines = text
        .lines()
        .filter_map(|line| line.strip_prefix(RECEIPT_PREFIX));
    let encoded = lines.next().ok_or_else(invalid)?;
    if lines.next().is_some() {
        return Err(invalid());
    }
    let value: Value = serde_json::from_str(encoded).map_err(|_| invalid())?;
    if value["schema_version"].as_u64() != Some(1)
        || value["verified"].as_bool() != Some(true)
        || value["daemon_pid"].as_u64() != Some(u64::from(root.pid))
        || value["daemon_start_time"].as_u64() != Some(root.started)
        || value["scope"].as_str() != Some("known-process-tree")
        || value["descendants"].as_u64().is_none()
    {
        return Err(invalid());
    }
    Ok(())
}

async fn bounded_snapshot(deadline: Instant) -> Result<Snapshot, String> {
    tokio::time::timeout_at(deadline, tokio::task::spawn_blocking(snapshot))
        .await
        .map_err(|_| "부서 종료 확인 시간이 초과됐습니다. 잠시 후 다시 시도하세요.".to_string())?
        .map_err(|e| format!("부서 종료 확인 작업에 실패했습니다: {e}"))?
}

pub async fn run(command: Command, mut tree: ProcessTree, deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        return Err("부서 종료 확인 시간이 초과됐습니다. 잠시 후 다시 시도하세요.".into());
    }
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let output_future = command.output();
    tokio::pin!(output_future);
    // Observe descendants while the helper is in flight, rather than taking only
    // a before/after snapshot. The receipt covers the approved known-tree scope;
    // neither polling nor the receipt claims arbitrary process containment.
    let output = loop {
        tokio::select! {
            result = &mut output_future => break result.map_err(|e| format!("부서 종료 명령을 실행하지 못했습니다: {e}"))?,
            _ = tokio::time::sleep_until(deadline) => return Err("부서 종료 명령의 대기 시간이 초과됐습니다. 잠시 후 다시 시도하세요.".into()),
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                let current = bounded_snapshot(deadline).await?;
                tree.remaining(&current)?;
            }
        }
    };
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "부서 종료에 실패했습니다 (종료 코드 {}). {}",
            output.status.code().unwrap_or(-1),
            detail.trim()
        ));
    }
    verify_receipt(&output.stdout, &tree.root)?;
    loop {
        let current = bounded_snapshot(deadline).await?;
        if Instant::now() >= deadline {
            return Err("부서 종료 확인 시간이 초과됐습니다. 잠시 후 다시 시도하세요.".into());
        }
        if !tree.remaining(&current)? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(
                "부서 프로세스가 남아 있어 종료를 확인하지 못했습니다. 잠시 후 다시 시도하세요."
                    .into(),
            );
        }
        tokio::time::sleep_until((Instant::now() + Duration::from_millis(100)).min(deadline)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Stdio};
    use std::time::Duration;

    #[test]
    fn child_fixture() {
        match std::env::var("CYS_DEPT_TEARDOWN_TEST_CHILD")
            .ok()
            .as_deref()
        {
            Some("exit7") => {
                eprintln!("stub teardown failure");
                std::process::exit(7);
            }
            Some("delayed") => std::thread::sleep(Duration::from_millis(400)),
            Some("hang") => std::thread::sleep(Duration::from_secs(5)),
            _ => (),
        }
        if let Ok(receipt) = std::env::var("CYS_DEPT_TEARDOWN_TEST_RECEIPT") {
            println!("{RECEIPT_PREFIX}{receipt}");
        }
    }

    fn command(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "dept_teardown::tests::child_fixture",
                "--nocapture",
            ])
            .env("CYS_DEPT_TEARDOWN_TEST_CHILD", mode)
            .stdin(Stdio::null());
        command
    }
    fn receipt(root: &Identity) -> Value {
        serde_json::json!({"schema_version":1,"verified":true,"daemon_pid":root.pid,"daemon_start_time":root.started,"scope":"known-process-tree","descendants":0})
    }

    fn verified_command(tree: &ProcessTree) -> Command {
        let mut cmd = command("success");
        cmd.env(
            "CYS_DEPT_TEARDOWN_TEST_RECEIPT",
            receipt(&tree.root).to_string(),
        );
        cmd
    }

    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if self.0.try_wait().unwrap().is_none() {
                self.0.kill().unwrap();
            }
            self.0.wait().unwrap();
        }
    }
    fn child(mode: &str) -> OwnedChild {
        OwnedChild(
            command(mode)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }
    fn empty_tree() -> ProcessTree {
        ProcessTree {
            root: Identity {
                pid: 42,
                started: 100,
            },
            identities: Vec::new(),
        }
    }

    #[test]
    fn teardown_propagates_helper_status_and_stderr() {
        tauri::async_runtime::block_on(async {
            let error = run(
                command("exit7"),
                empty_tree(),
                Instant::now() + Duration::from_secs(2),
            )
            .await
            .unwrap_err();
            assert!(
                error.contains('7') && error.contains("stub teardown failure"),
                "{error}"
            );
            assert!(!error.is_ascii());
        });
    }

    #[test]
    fn teardown_propagates_spawn_error() {
        tauri::async_runtime::block_on(async {
            let missing =
                std::env::temp_dir().join(format!("missing-dept-command-{}", std::process::id()));
            let error = run(
                Command::new(missing),
                empty_tree(),
                Instant::now() + Duration::from_secs(2),
            )
            .await
            .unwrap_err();
            assert!(error.contains("실행"), "{error}");
        });
    }

    #[test]
    fn teardown_waits_for_captured_process_after_helper_success() {
        let owned = child("delayed");
        let tree = ProcessTree::capture(&[owned.0.id()]).unwrap();
        let start = std::time::Instant::now();
        tauri::async_runtime::block_on(async {
            run(
                verified_command(&tree),
                tree,
                Instant::now() + Duration::from_secs(2),
            )
            .await
            .unwrap();
        });
        assert!(
            start.elapsed() >= Duration::from_millis(100),
            "premature status0 must not prove process death"
        );
    }

    #[test]
    fn teardown_rejects_old_helper_success_with_surviving_process() {
        let mut owned = child("hang");
        let tree = ProcessTree::capture(&[owned.0.id()]).unwrap();
        tauri::async_runtime::block_on(async {
            let error = run(
                command("success"),
                tree,
                Instant::now() + Duration::from_millis(80),
            )
            .await
            .unwrap_err();
            assert!(error.contains("종료") && error.contains("확인"), "{error}");
        });
        assert!(
            owned.0.try_wait().unwrap().is_none(),
            "bridge must not kill an unverified process itself"
        );
    }

    #[test]
    fn teardown_valid_receipt_still_waits_for_surviving_identity() {
        let mut owned = child("hang");
        let tree = ProcessTree::capture(&[owned.0.id()]).unwrap();
        tauri::async_runtime::block_on(async {
            let error = run(
                verified_command(&tree),
                tree,
                Instant::now() + Duration::from_millis(80),
            )
            .await
            .unwrap_err();
            assert!(error.contains("시간"), "{error}");
        });
        assert!(owned.0.try_wait().unwrap().is_none());
    }

    #[test]
    fn teardown_expired_deadline_never_starts_the_command() {
        tauri::async_runtime::block_on(async {
            let missing = std::env::temp_dir().join("missing-dept-expired-command");
            let error = run(
                Command::new(missing),
                empty_tree(),
                Instant::now() - Duration::from_millis(1),
            )
            .await
            .unwrap_err();
            assert!(
                error.contains("시간"),
                "expired budget must be rejected before spawn: {error}"
            );
        });
    }

    #[test]
    fn teardown_bounds_a_hung_helper() {
        let start = std::time::Instant::now();
        tauri::async_runtime::block_on(async {
            let error = run(
                command("hang"),
                empty_tree(),
                Instant::now() + Duration::from_millis(80),
            )
            .await
            .unwrap_err();
            assert!(error.contains("시간"), "{error}");
        });
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn teardown_does_not_wait_for_a_reused_pid_identity() {
        let mut owned = child("hang");
        let mut tree = ProcessTree::capture(&[owned.0.id()]).unwrap();
        assert!(!tree.identities.is_empty());
        for id in &mut tree.identities {
            id.started += 1;
        }
        tauri::async_runtime::block_on(async {
            run(
                verified_command(&tree),
                tree,
                Instant::now() + Duration::from_secs(2),
            )
            .await
            .unwrap();
        });
        assert!(owned.0.try_wait().unwrap().is_none());
    }

    #[test]
    fn teardown_rejects_an_unavailable_process_snapshot() {
        assert!(snapshot_from_system(&System::new()).is_err());
    }

    #[test]
    fn teardown_captures_rpc_daemon_and_live_shell_ids() {
        assert_eq!(process_ids(&serde_json::json!({"daemon_pid":42}), &serde_json::json!({"surfaces":[{"pid":43,"exited":false},{"pid":44,"exited":true}]})).unwrap(), vec![42,43]);
        assert!(process_ids(
            &serde_json::json!({"daemon_pid":0}),
            &serde_json::json!({"surfaces":[]})
        )
        .is_err());
        assert!(process_ids(
            &serde_json::json!({"daemon_pid":42}),
            &serde_json::json!({})
        )
        .is_err());
    }

    #[test]
    fn teardown_discovers_orphan_with_dead_parent_link() {
        let mut tree = ProcessTree {
            root: Identity {
                pid: 42,
                started: 100,
            },
            identities: vec![Identity {
                pid: 42,
                started: 100,
            }],
        };
        let after_helper = Snapshot::from([(43, (Some(42), 101))]);
        assert!(
            tree.remaining(&after_helper).unwrap(),
            "old helper zero cannot hide a newly orphaned child"
        );
        assert!(tree.identities.contains(&Identity {
            pid: 43,
            started: 101
        }));
    }

    #[test]
    fn teardown_unknown_start_time_fails_closed() {
        let mut tree = ProcessTree {
            root: Identity {
                pid: 42,
                started: 100,
            },
            identities: vec![Identity {
                pid: 42,
                started: 100,
            }],
        };
        assert!(tree.remaining(&Snapshot::from([(42, (None, 0))])).is_err());
    }

    #[test]
    fn teardown_missing_receipt_is_visible_failure() {
        tauri::async_runtime::block_on(async {
            let error = run(
                command("success"),
                empty_tree(),
                Instant::now() + Duration::from_secs(2),
            )
            .await
            .unwrap_err();
            assert!(error.contains(".new") && error.contains("종료"), "{error}");
        });
    }

    #[test]
    fn teardown_rejects_invalid_or_unbound_receipts() {
        let root = Identity {
            pid: 42,
            started: 100,
        };
        let valid = receipt(&root);
        let format = |value: &Value| format!("{RECEIPT_PREFIX}{value}\n");
        for (field, bad) in [
            ("schema_version", serde_json::json!(2)),
            ("verified", serde_json::json!(false)),
            ("daemon_pid", serde_json::json!(43)),
            ("daemon_start_time", serde_json::json!(101)),
            ("scope", serde_json::json!("all-processes")),
            ("descendants", serde_json::json!(-1)),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = bad;
            assert!(
                verify_receipt(format(&invalid).as_bytes(), &root).is_err(),
                "field {field}"
            );
        }
        assert!(verify_receipt(b"CYS_DEPT_STOP_RESULT {broken}", &root).is_err());
        assert!(verify_receipt(
            format!("{}{}", format(&valid), format(&valid)).as_bytes(),
            &root
        )
        .is_err());
    }

    #[test]
    fn teardown_accepts_only_identity_bound_verified_receipt() {
        let root = Identity {
            pid: 42,
            started: 100,
        };
        let stdout = format!("unrelated output\n{RECEIPT_PREFIX}{}\n", receipt(&root));
        assert!(verify_receipt(stdout.as_bytes(), &root).is_ok());
    }

    #[test]
    fn teardown_rejects_invalid_capture_identity() {
        assert!(
            ProcessTree::capture(&[u32::MAX]).is_err(),
            "a missing daemon identity must fail closed"
        );
        assert!(
            ProcessTree::capture(&[0]).is_err(),
            "an unavailable identity is not proof of an empty tree"
        );
    }
}
