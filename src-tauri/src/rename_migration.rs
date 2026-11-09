//! Executable-name cutover. Every path is serialized before normal app autostart.
use cys::rename_handover::ProcessIdentity;
use serde_json::{json, Value};

#[cfg(any(target_os="macos",windows))]
#[path="rename_migration_native.rs"]
mod native;

pub(crate) async fn run(settings_file: std::path::PathBuf) -> Result<Outcome,String> {
    #[cfg(any(target_os="macos",windows))] { native::run(settings_file).await }
    #[cfg(not(any(target_os="macos",windows)))] { let _ = settings_file; Ok(Outcome::AlreadyReady) }
}

pub(crate) async fn validate_layout_owner(marker: &Value,settings_file:std::path::PathBuf) -> Result<(),String> {
    #[cfg(any(target_os="macos",windows))] { native::validate_layout_owner(marker,settings_file).await }
    #[cfg(not(any(target_os="macos",windows)))] { let _ = (marker,settings_file); Err("이 운영체제의 좌석 이관 검증은 지원하지 않습니다.".into()) }
}

/// Consume the zero-owner layout snapshot named by the marker, only when the frontend has acknowledged a durable layout.
/// `Ok` also when the marker carries no snapshot. `Err` keeps the marker so the caller can retry with a Korean notice.
pub(crate) fn consume_layout_snapshot_with(record: &Value, consume: impl FnOnce(&cys::update_snapshot::Reference) -> Result<(),String>) -> Result<(),String> {
    let reference = &record["snapshot"]["layout_snapshot"]["reference"];
    if reference.is_null() { return Ok(()); }
    let reference: cys::update_snapshot::Reference = serde_json::from_value(reference.clone())
        .map_err(|_| "배치 기록 참조를 읽지 못해 복원 기록을 유지합니다.".to_string())?;
    consume(&reference)
}

#[cfg(target_os="macos")]
pub(crate) fn repair_cli_paths() -> Result<(),String> { native::repair_cli_paths() }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Owner {
    identity: ProcessIdentity,
    version: String,
    canonical: bool,
}

#[derive(Clone, Debug, Default)]
struct Observation {
    owner: Option<Owner>,
    legacy_registration: bool,
    canonical_registration: bool,
    uncertain: bool,
}

trait Backend {
    async fn inspect(&mut self) -> Result<Observation, String>;
    async fn capture(&mut self, before: &Observation) -> Result<Value, String>;
    async fn record(&mut self, phase: &str, before: &Observation, snapshot: &Value) -> Result<(), String>;
    async fn prepare(&mut self, snapshot: &Value) -> Result<(), String>;
    async fn checkpoint(&mut self, before: &Observation, snapshot: &mut Value) -> Result<(), String>;
    async fn disable_legacy(&mut self, snapshot: &Value) -> Result<(), String>;
    async fn stop_old(&mut self, owner: &Owner) -> Result<(), String>;
    async fn wait_old_exit(&mut self, owner: &Owner) -> Result<(), String>;
    async fn activate(&mut self) -> Result<(), String>;
    async fn await_new(&mut self) -> Result<Owner, String>;
    async fn restore(&mut self, snapshot: &Value, owner: &Owner) -> Result<(), String>;
    async fn finish(&mut self) -> Result<(), String>;
    async fn rollback(&mut self, snapshot: &Value, before: &Observation) -> Result<(), String>;
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome { AlreadyReady, Migrated }

async fn perform<B: Backend>(backend: &mut B) -> Result<Outcome, String> {
    let before = backend.inspect().await?;
    if before.uncertain {
        return Err("실행 중인 프로세스나 자동 시작 등록을 확인하지 못했습니다. 기존 상태를 유지합니다.".into());
    }
    if before.owner.as_ref().is_none_or(|owner| owner.canonical && owner.version == env!("CARGO_PKG_VERSION"))
        && before.canonical_registration && !before.legacy_registration {
        return Ok(Outcome::AlreadyReady);
    }
    let mut snapshot = backend.capture(&before).await?;
    let result: Result<Outcome, String> = async {
        backend.record("prepared", &before, &snapshot).await?;
        backend.prepare(&snapshot).await?;
        if before.owner.is_some() {
            backend.checkpoint(&before, &mut snapshot).await?;
        }
        backend.record("stopping-old", &before, &snapshot).await?;
        backend.disable_legacy(&snapshot).await?;
        if let Some(owner) = &before.owner {
            backend.stop_old(owner).await?;
            backend.wait_old_exit(owner).await?;
        }
        backend.record("starting-new", &before, &snapshot).await?;
        backend.activate().await?;
        let owner = backend.await_new().await?;
        if !owner.canonical || owner.version != env!("CARGO_PKG_VERSION")
            || before.owner.as_ref().is_some_and(|old| old.identity == owner.identity) {
            return Err("새 실행 파일의 소유권과 버전을 확인하지 못했습니다. 복원을 중단합니다.".into());
        }
        backend.record("restoring", &before, &snapshot).await?;
        backend.restore(&snapshot, &owner).await?;
        backend.finish().await?;
        Ok(Outcome::Migrated)
    }.await;
    match result {
        Ok(outcome) => Ok(outcome),
        Err(error) => match backend.rollback(&snapshot, &before).await {
            Ok(()) => Err(error),
            Err(rollback) => Err(format!("{error}\n기존 상태 복구도 완료하지 못했습니다: {rollback}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        seen: Observation,
        events: Vec<String>,
        fail: &'static str,
        rollback_snapshot: Option<Value>,
        rollback_fail: bool,
    }
    impl Fake {
        fn old() -> Self {
            Self {seen: Observation {owner: Some(Owner {identity: ProcessIdentity {
                pid: 7, start_time: 12, executable: "legacy".into()}, version: "0.3.1".into(), canonical: false}),
                legacy_registration: true, canonical_registration: false, uncertain: false}, events: vec![], fail: "",
                rollback_snapshot: None, rollback_fail: false}
        }
        fn event(&mut self, name: &str) -> Result<(), String> {
            self.events.push(name.into());
            if self.fail == name { Err(format!("실패: {name}")) } else { Ok(()) }
        }
    }
    impl Backend for Fake {
        async fn inspect(&mut self) -> Result<Observation, String> {self.event("inspect")?; Ok(self.seen.clone())}
        async fn capture(&mut self, _: &Observation) -> Result<Value, String> {self.event("capture")?;Ok(json!({"preserved":true}))}
        async fn record(&mut self, phase: &str, _: &Observation, _: &Value) -> Result<(), String> {self.event(phase)}
        async fn prepare(&mut self, _: &Value) -> Result<(), String> {self.event("prepare")}
        async fn checkpoint(&mut self, _: &Observation, s: &mut Value) -> Result<(), String> {self.event("checkpoint")?;s["checkpoint"]=json!({"all_saved":true});Ok(())}
        async fn disable_legacy(&mut self, _: &Value) -> Result<(), String> {self.event("disable-legacy")}
        async fn stop_old(&mut self, _: &Owner) -> Result<(), String> {self.event("stop-old")}
        async fn wait_old_exit(&mut self, _: &Owner) -> Result<(), String> {self.event("wait-old")}
        async fn activate(&mut self) -> Result<(), String> {self.event("activate")}
        async fn await_new(&mut self) -> Result<Owner, String> {self.event("await-new")?;Ok(Owner {
            identity: ProcessIdentity {pid:8,start_time:13,executable:"canonical".into()},
            version:env!("CARGO_PKG_VERSION").into(),canonical:true})}
        async fn restore(&mut self, _: &Value, _: &Owner) -> Result<(), String> {self.event("restore")}
        async fn finish(&mut self) -> Result<(), String> {self.event("finish")}
        async fn rollback(&mut self, s: &Value, _: &Observation) -> Result<(), String> {
            self.rollback_snapshot=Some(s.clone());self.event("rollback")?;
            if self.rollback_fail {Err("복구 실패".into())} else {Ok(())}
        }
    }

    #[tokio::test]
    async fn cli_started_old_owner_and_idle_legacy_job_have_one_ordered_cutover() {
        let mut fake=Fake::old();
        assert_eq!(perform(&mut fake).await.unwrap(),Outcome::Migrated);
        assert_eq!(fake.events, ["inspect","capture","prepared","prepare","checkpoint","stopping-old",
            "disable-legacy","stop-old","wait-old","starting-new","activate","await-new","restoring","restore","finish"]);
    }
    #[tokio::test]
    async fn failed_checkpoint_never_stops_or_starts_and_rolls_back() {
        let mut fake=Fake::old();fake.fail="checkpoint";
        assert!(perform(&mut fake).await.unwrap_err().contains("checkpoint"));
        assert!(!fake.events.iter().any(|e| ["stop-old","activate"].contains(&e.as_str())));
        assert_eq!(fake.events.last().unwrap(),"rollback");
    }
    #[tokio::test]
    async fn uncertain_old_owner_never_prepares_or_spawns() {
        let mut fake=Fake::old();fake.seen.uncertain=true;
        assert!(perform(&mut fake).await.is_err());assert_eq!(fake.events,["inspect"]);
    }
    #[tokio::test]
    async fn missing_exit_proof_never_activates_new_owner() {
        let mut fake=Fake::old();fake.fail="wait-old";
        assert!(perform(&mut fake).await.is_err());assert!(!fake.events.contains(&"activate".into()));
    }
    #[tokio::test]
    async fn permission_or_restore_failure_is_not_ready() {
        for fail in ["prepare","await-new","restore"] {
            let mut fake=Fake::old();fake.fail=fail;
            assert!(perform(&mut fake).await.is_err());assert!(!fake.events.contains(&"finish".into()));
            assert_eq!(fake.events.last().unwrap(),"rollback");
        }
    }
    #[tokio::test]
    async fn canonical_registration_waiting_for_start_is_not_migrated_again() {
        let mut fake=Fake::old();fake.seen.owner=None;fake.seen.legacy_registration=false;fake.seen.canonical_registration=true;
        assert_eq!(perform(&mut fake).await.unwrap(),Outcome::AlreadyReady);
        assert_eq!(fake.events,["inspect"]);
    }
    #[tokio::test]
    async fn canonical_owner_and_registration_is_an_idempotent_noop() {
        let mut fake=Fake::old();fake.seen.legacy_registration=false;fake.seen.canonical_registration=true;
        let owner=fake.seen.owner.as_mut().unwrap();owner.canonical=true;owner.version=env!("CARGO_PKG_VERSION").into();
        assert_eq!(perform(&mut fake).await.unwrap(),Outcome::AlreadyReady);assert_eq!(fake.events,["inspect"]);
    }
    #[tokio::test]
    async fn rollback_gets_the_checkpointed_snapshot_so_seat_recovery_can_be_gated() {
        let mut fake=Fake::old();fake.fail="activate";
        assert!(perform(&mut fake).await.is_err());
        assert_eq!(fake.rollback_snapshot.unwrap()["checkpoint"]["all_saved"],true);
        let mut early=Fake::old();early.fail="checkpoint";
        assert!(perform(&mut early).await.is_err());
        assert!(early.rollback_snapshot.unwrap()["checkpoint"].is_null());
    }
    #[tokio::test]
    async fn rollback_failure_keeps_both_the_original_and_the_recovery_error() {
        let mut fake=Fake::old();fake.fail="activate";fake.rollback_fail=true;
        let error=perform(&mut fake).await.unwrap_err();
        assert!(error.contains("실패: activate")&&error.contains("복구 실패"),"{error}");
    }
}

#[cfg(test)]
mod consume_tests {
    use super::*;
    fn record(with: bool) -> Value {
        let layout = if with { json!({"reference":{"path":"/p/.pending-rename-layout.json","id":"0".repeat(32),"digest":"1".repeat(64)},"drain_status":null}) } else { Value::Null };
        json!({"snapshot":{"layout_snapshot":layout,"notice":null}})
    }
    #[test]
    fn consume_runs_once_with_the_recorded_reference_after_ack() {
        let mut seen = None;
        consume_layout_snapshot_with(&record(true), |r| { seen = Some(r.clone()); Ok(()) }).unwrap();
        assert_eq!(seen.unwrap().id, "0".repeat(32));
    }
    #[test]
    fn no_snapshot_means_nothing_to_consume() {
        consume_layout_snapshot_with(&record(false), |_| panic!("must not consume")).unwrap();
    }
    #[test]
    fn consume_failure_is_returned_so_the_marker_is_kept() {
        assert!(consume_layout_snapshot_with(&record(true), |_| Err("실패".into())).is_err());
    }
    #[test]
    fn malformed_reference_fails_closed_without_consuming() {
        let bad = json!({"snapshot":{"layout_snapshot":{"reference":{"path":1}}}});
        assert!(consume_layout_snapshot_with(&bad, |_| panic!("must not consume")).is_err());
    }
}
