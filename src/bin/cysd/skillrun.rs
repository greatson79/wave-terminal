//! CC v2 WS-B: 스킬보드 실행 생애주기 — `cys skill run --run-id`가 등록(skill.run_started)한
//! run을 워처가 **산출물 + 데드라인**으로 전이시킨다(launched→done|failed).
//!
//! surface 결합을 의도적으로 하지 않는다: skill run은 스케줄러 경유 fresh 원샷이라(cys.rs
//! SkillAction::Run — schedule add --fresh) 실행 시점에 surface_id가 없다. 스케줄러에
//! 역결합을 만드는 대신 결정론 신호 2개로 판정한다:
//!   done   = 산출물 dir(~/.cys/_round/skill-out/<run_id>/)이 비어있지 않음 (티켓 out_fmt이 핀)
//!   failed = 데드라인(started_at + ttl + grace) 초과에도 산출물 없음 (사유 timeout_no_artifact)
//! 데몬 재시작 시 열린 run은 failed(daemon_restart) — fresh pane은 phoenix 복원 대상이 아니다.

use crate::state::Daemon;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

const WATCH_TICK_SECS: u64 = 5;
/// 데드라인 여유(초) — fresh surface TTL 이후 산출물 flush·회수 지연 허용.
const DEADLINE_GRACE_SECS: f64 = 120.0;
/// ttl 미보고 시 기본(초) — cys.rs skill run의 close_after 기본 600과 정합.
const DEFAULT_TTL_SECS: f64 = 600.0;

/// run 산출물 결정론 위치 — make_ticket(out_fmt)·skill_out_dir(Tauri)와 같은 SOT.
pub fn artifact_dir(run_id: &str) -> Option<std::path::PathBuf> {
    Some(dirs::home_dir()?.join(".cys/_round/skill-out").join(run_id))
}

/// 산출물 안정화 대기(초) — 워커가 dir·부분 파일을 만들고 아직 쓰는 중에 done 오판 방지.
const ARTIFACT_SETTLE_SECS: f64 = 30.0;

/// 산출물 관측 — (존재 여부, 안정화 여부). 안정화 = 가장 최근 mtime이 settle 창 이전.
fn artifact_state(run_id: &str, now: f64) -> (bool, bool) {
    let Some(dir) = artifact_dir(run_id) else { return (false, false) };
    let Ok(entries) = std::fs::read_dir(&dir) else { return (false, false) };
    let mut any = false;
    let mut newest = 0.0f64;
    for e in entries.flatten() {
        any = true;
        if let Some(m) = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
        {
            if m > newest {
                newest = m;
            }
        }
    }
    (any, any && now - newest >= ARTIFACT_SETTLE_SECS)
}

/// skill.run_started RPC 본문 — analytics에 launched 기록.
pub fn run_started(daemon: &Arc<Daemon>, params: &Value) -> Result<Value, String> {
    let run_id = params
        .get("run_id")
        .and_then(|x| x.as_str())
        .filter(|s| {
            !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
        .ok_or("invalid run_id")?; // 경로 성분 금지(a-z0-9-_) — artifact_dir join 안전
    let name = params.get("name").and_then(|x| x.as_str()).unwrap_or("");
    let label = params.get("label").and_then(|x| x.as_str()).unwrap_or(name);
    let ttl = params
        .get("ttl_secs")
        .and_then(|x| x.as_f64())
        .unwrap_or(DEFAULT_TTL_SECS)
        .clamp(30.0, 86400.0);
    let now = crate::state::now_epoch();
    let deadline = now + ttl + DEADLINE_GRACE_SECS;
    let guard = daemon.analytics.lock().unwrap();
    let conn = guard.as_ref().ok_or("analytics unavailable")?;
    crate::analytics::skill_run_insert(conn, run_id, name, label, now, deadline);
    Ok(json!({"run_id": run_id, "deadline": deadline}))
}

/// skill.runs RPC 본문 — 최근 run 목록.
pub fn runs_list(daemon: &Arc<Daemon>, params: &Value) -> Value {
    let limit = params.get("limit").and_then(|x| x.as_u64()).unwrap_or(20).min(100);
    let guard = daemon.analytics.lock().unwrap();
    match guard.as_ref() {
        Some(conn) => json!({"runs": crate::analytics::skill_runs_list(conn, limit)}),
        None => json!({"runs": []}),
    }
}

/// 부트 reconcile — 이전 데몬의 열린 run은 fresh pane과 함께 소멸 → failed(daemon_restart).
pub fn reconcile_boot(daemon: &Arc<Daemon>) {
    let now = crate::state::now_epoch();
    let guard = daemon.analytics.lock().unwrap();
    if let Some(conn) = guard.as_ref() {
        crate::analytics::skill_runs_fail_open(conn, "daemon_restart", now);
    }
}

/// 전이 워처 — 5초 틱: 산출물 존재→done · 데드라인 초과→failed(timeout_no_artifact).
pub fn spawn_watcher(daemon: Arc<Daemon>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(WATCH_TICK_SECS)).await;
            let open: Vec<(String, f64)> = {
                let guard = daemon.analytics.lock().unwrap();
                match guard.as_ref() {
                    Some(conn) => crate::analytics::skill_runs_open(conn),
                    None => continue,
                }
            };
            if open.is_empty() {
                continue;
            }
            let now = crate::state::now_epoch();
            for (run_id, deadline) in open {
                // settled=done(안정화된 산출물) · 마감 시 산출물 있으면 done(안정화 미달이라도
                // 수용 — 마감 직전 산출물을 timeout 오판하지 않는다) · 마감+무산출=failed.
                let (any, settled) = artifact_state(&run_id, now);
                let (next, note): (&str, Option<&str>) = if settled {
                    ("done", None)
                } else if now > deadline && any {
                    ("done", None)
                } else if now > deadline {
                    ("failed", Some("timeout_no_artifact"))
                } else {
                    continue;
                };
                let art = if next == "done" {
                    artifact_dir(&run_id).map(|d| d.to_string_lossy().into_owned())
                } else {
                    None
                };
                {
                    let guard = daemon.analytics.lock().unwrap();
                    if let Some(conn) = guard.as_ref() {
                        crate::analytics::skill_run_update(
                            conn,
                            &run_id,
                            next,
                            note,
                            art.as_deref(),
                            Some(now),
                        );
                    }
                }
                daemon.bus.publish(
                    "skill.run_finished",
                    "skill",
                    None,
                    json!({"run_id": run_id, "status": next, "artifact_dir": art}),
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_started_is_listed_and_rejects_path_components() {
        let dir = std::env::temp_dir().join(format!("cys-skillrun-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let daemon = Daemon::new(dir.join("cys.sock"));
        assert!(run_started(&daemon, &json!({"run_id": "../escape"})).is_err());
        run_started(&daemon, &json!({"run_id": "safe-1", "name": "smoke"})).unwrap();
        assert_eq!(runs_list(&daemon, &json!({}))["runs"][0]["status"], "launched");
        drop(daemon);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
