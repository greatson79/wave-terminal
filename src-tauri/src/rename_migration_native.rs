//! Production adapter. Tests exercise the coordinator through its in-memory backend.
use super::{Backend, Observation, Outcome, Owner, perform};
use cys::rename_handover;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(target_os="macos")]
#[path="rename_migration_macos.rs"]
mod platform;
#[cfg(windows)]
#[path="rename_migration_windows.rs"]
mod platform;

struct Native {
    socket: PathBuf,
    cli: PathBuf,
    daemon: PathBuf,
    settings_file: PathBuf,
    platform: Value,
    marker: Value,
    new_owner: Option<Owner>,
    surface_map: serde_json::Map<String, Value>,
}

async fn request(socket: &Path, method: &str, params: Value) -> Result<Value,String> {
    tokio::time::timeout(Duration::from_secs(5),crate::rpc_oneshot(socket,method,params)).await
        .map_err(|_| format!("업데이트 상태 확인 시간이 초과됐습니다: {method}"))?
}

async fn socket_present(socket: &Path) -> Result<bool,String> {
    #[cfg(unix)]
    let result = tokio::time::timeout(Duration::from_secs(2),tokio::net::UnixStream::connect(socket)).await
        .map_err(|_| "기존 서비스의 연결 상태를 확인하지 못했습니다.".to_string())?;
    #[cfg(windows)]
    let result = tokio::net::windows::named_pipe::ClientOptions::new().open(socket);
    match result {
        Ok(_) => Ok(true),
        Err(error) if matches!(error.kind(),std::io::ErrorKind::NotFound|std::io::ErrorKind::ConnectionRefused) => Ok(false),
        Err(error) => Err(format!("기존 서비스 연결 상태가 불확실합니다. 새 프로세스를 시작하지 않습니다: {error}")),
    }
}

/// Previous-release daemon inside THIS bundle dir: exact file names only (shipped `cysd`/`cysd.exe`
/// and the NSIS WaveSwap renames `cysd.prev.exe`..`prev3.exe`). Never prefix/contains/PATH matching.
/// ponytail: byte-exact path compare; Windows case/`\\?\` differences fail closed (owner kept, not killed).
fn is_previous_bundle_daemon(executable:&Path,base:&Path)->bool{
    const NAMES:[&str;5]=["cysd","cysd.exe","cysd.prev.exe","cysd.prev2.exe","cysd.prev3.exe"];
    let name=executable.file_name().and_then(|n|n.to_str()).unwrap_or("");
    base.is_absolute() && executable.parent()==Some(base) && NAMES.contains(&name)
}

#[derive(Debug,PartialEq,Eq)]
enum RollbackPlan { Clear, KeepMarker, RecoverSeats }

/// What to do after the OS rollback attempt. Pure; the OS error stays data until this is decided.
/// `links_ok`: Mac `links_rolled_back` (always true where there are no links, i.e. Windows).
fn rollback_plan(had_owner:bool,original_serving:bool,platform_ok:bool,registration_restored:bool,links_ok:bool,all_saved:bool)->RollbackPlan{
    if !had_owner || original_serving {
        return if platform_ok {RollbackPlan::Clear} else {RollbackPlan::KeepMarker};
    }
    // Original owner is gone: its seats come back only from a verified checkpoint and a safe OS receipt.
    if all_saved && (platform_ok || (registration_restored && links_ok)) {RollbackPlan::RecoverSeats} else {RollbackPlan::KeepMarker}
}

#[derive(Debug,PartialEq,Eq)]
enum ResumePlan { Refuse, StartRestored, Recover }

/// Restart with a durable rollback-recovery receipt: never forward activation. Pure.
fn resume_plan(platform:&Value,all_saved:bool,had_owner:bool,original_alive:bool,owner_present:bool,links_ok:bool)->ResumePlan{
    if original_alive { return ResumePlan::Refuse; }
    if rollback_plan(had_owner,false,platform["rollback_succeeded"]==true,
        platform["old_registration_restored"]==true,links_ok,all_saved)!=RollbackPlan::RecoverSeats {
        return ResumePlan::Refuse;
    }
    if !owner_present && platform["restored_owner_start_requested"]!=true {ResumePlan::StartRestored} else {ResumePlan::Recover}
}

/// `record()` rebuilds the marker; the rollback receipt and diagnostic must survive every rebuild.
fn carry_rollback_receipt(previous:&Value,next:&mut Value){
    for key in ["rollback_recovery_pending","rollback_diagnostic"] {
        if let Some(value)=previous["rename_handover"].get(key) { next["rename_handover"][key]=value.clone(); }
    }
}

/// The unresolved registration cleanup stays visible through the layout notice the UI already shows.
fn with_rollback_notice(snapshot:&Value,diagnostic:Option<&str>)->Value{
    let mut snapshot=snapshot.clone();
    let Some(diagnostic)=diagnostic else { return snapshot; };
    let line=format!("기존 좌석은 복구했지만 자동 시작 등록 정리는 끝나지 않았습니다: {diagnostic}");
    let notice=match snapshot["notice"].as_str() {
        Some(existing) if existing.contains(&line) => return snapshot,
        Some(existing) if !existing.is_empty() => format!("{existing}\n{line}"),
        _ => line,
    };
    snapshot["notice"]=json!(notice);
    snapshot
}

impl Native {
    /// Verified-owner → restore → finish; the marker is never cleared here (layout ack completes it).
    async fn recover_seats(&mut self,snapshot:&Value,before:&Observation)->Result<(),String>{
        let snapshot=&with_rollback_notice(snapshot,self.marker["rename_handover"]["rollback_diagnostic"].as_str());
        let mut found=None;
        for _ in 0..300 {
            if let Some(owner)=self.owner().await? {found=Some(owner);break;}
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let owner=found.ok_or("등록과 명령 연결은 복구했지만 기존 좌석의 재시작 확인이 필요합니다. 복원 기록을 유지합니다.")?;
        if !owner.canonical||owner.version!=env!("CARGO_PKG_VERSION"){return Err("복구된 서비스의 실행 경로와 버전을 확인하지 못했습니다. 복원 기록을 유지합니다.".into());}
        if self.new_owner.as_ref().is_some_and(|previous|previous.identity!=owner.identity){self.surface_map.clear();}
        self.new_owner=Some(owner.clone());
        self.record("restoring",before,snapshot).await?;
        self.restore(snapshot,&owner).await?;
        self.finish().await
    }
    fn new(settings_file: PathBuf) -> Result<Self,String> {
        let exe=std::env::current_exe().map_err(|e| format!("앱 실행 경로 확인 실패: {e}"))?;
        let base=exe.parent().ok_or("앱 실행 폴더가 없습니다.")?;
        let cli=base.join(if cfg!(windows){"wave.exe"}else{"wave"});
        let daemon=base.join(if cfg!(windows){"waved.exe"}else{"waved"});
        if !cli.is_file() || !daemon.is_file() { return Err("업데이트에 필요한 실행 파일이 앱에 없습니다. 수동 설치 안내를 확인하세요.".into()); }
        if !settings_file.is_absolute(){return Err("UI 설정의 절대 경로를 확인하지 못했습니다.".into());}
        Ok(Self{socket:crate::default_socket(),cli,daemon,settings_file,platform:Value::Null,marker:Value::Null,
            new_owner:None,surface_map:serde_json::Map::new()})
    }

    async fn owner(&self) -> Result<Option<Owner>,String> {
        if !socket_present(&self.socket).await? { return Ok(None); }
        let identify=request(&self.socket,"system.identify",json!({"caller":"ui"})).await?;
        if identify["socket_path"].as_str() != self.socket.to_str() { return Err("응답한 서비스의 소켓이 다릅니다.".into()); }
        let pid=identify["daemon_pid"].as_u64().and_then(|n|u32::try_from(n).ok()).filter(|n|*n>1)
            .ok_or("기존 프로세스 번호를 확인하지 못했습니다.")?;
        let identity=rename_handover::process_identity(pid)?.ok_or("기존 프로세스가 상태 확인 중 종료됐습니다.")?;
        let version=identify["version"].as_str().filter(|s|!s.is_empty()).ok_or("기존 서비스 버전이 없습니다.")?.to_string();
        let canonical=identity.executable == self.daemon;
        let base=self.daemon.parent().unwrap();
        let old_bundle=is_previous_bundle_daemon(&identity.executable,base);
        let old_link=[Path::new("/opt/homebrew/bin/cysd"),Path::new("/usr/local/bin/cysd")]
            .contains(&identity.executable.as_path());
        if !canonical && !old_bundle && !old_link { return Err("다른 설치의 실행 프로세스는 종료하지 않습니다. 기존 서비스를 유지합니다.".into()); }
        let started=identify["started_at"].as_f64().filter(|n|n.is_finite()&&*n>0.0).ok_or("기존 서비스 시작 시각을 확인하지 못했습니다.")?;
        if identity.start_time as f64 > started { return Err("기존 서비스 응답과 프로세스 시작 시각이 다릅니다.".into()); }
        Ok(Some(Owner{identity,version,canonical}))
    }

    fn persist_surface_map(&mut self) -> Result<(),String> {
        self.marker["rename_handover"]["surface_map"]=json!(self.surface_map);
        rename_handover::write_marker(Some(&self.marker))
    }

    async fn same_owner(&self,expected:&Owner) -> Result<(),String> {
        if self.owner().await?.as_ref().map(|o|&o.identity)!=Some(&expected.identity) {
            return Err("서비스 소유권이 바뀌어 업데이트를 보류합니다.".into());
        }
        Ok(())
    }

    fn platform_change(&mut self, action: impl FnOnce(&mut Value,&mut dyn FnMut(&Value)->Result<(),String>)->Result<(),String>) -> Result<(),String> {
        let mut marker=self.marker.clone();
        let result=action(&mut self.platform,&mut |value| {
            marker["rename_handover"]["snapshot"]["platform"]=value.clone();
            rename_handover::write_marker(Some(&marker))
        });
        self.marker=marker;
        result
    }

    async fn cli(&self,args:&[&str]) -> Result<std::process::Output,String> {
        self.cli_input(args,None).await
    }

    async fn cli_input(&self,args:&[&str],input:Option<&Value>) -> Result<std::process::Output,String> {
        let bytes=input.map(serde_json::to_vec).transpose()
            .map_err(|_| "저장된 역할 복원 입력을 준비하지 못했습니다.".to_string())?;
        if bytes.as_ref().is_some_and(|bytes|bytes.len()>1024*1024) {
            return Err("저장된 역할 복원 입력이 1 MiB를 넘습니다. 기존 기록을 유지합니다.".into());
        }
        let mut command=tokio::process::Command::new(&self.cli);
        command.arg("--socket").arg(&self.socket).args(args).env("CYS_NO_AUTOSTART","1")
            .stdin(if bytes.is_some(){std::process::Stdio::piped()}else{std::process::Stdio::null()})
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
        #[cfg(windows)] command.creation_flags(0x08000000);
        let output=tokio::time::timeout(Duration::from_secs(120),async {
            let child=command.spawn().map_err(|_| "업데이트 저장 또는 복원 명령을 실행하지 못했습니다.".to_string())?;
            finish_cli_input(child,bytes).await
        }).await.map_err(|_| "업데이트 저장 또는 복원 명령 시간이 초과됐습니다.".to_string())??;
        if !output.status.success() { return Err("업데이트 저장 또는 복원 명령이 실패했습니다. 기존 기록을 유지합니다.".into()); }
        Ok(output)
    }

    async fn wait_gone(&self,owner:&Owner) -> Result<(),String> {
        for _ in 0..150 {
            if !rename_handover::identity_alive(&owner.identity)? {
                if self.owner().await?.is_none() { return Ok(()); }
                return Err("다른 프로세스가 소켓을 소유하고 있어 다음 서비스를 시작하지 않습니다.".into());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err("기존 프로세스 종료를 확인하지 못했습니다. 새 서비스를 시작하지 않습니다.".into())
    }
}


async fn finish_cli_input(mut child:tokio::process::Child,bytes:Option<Vec<u8>>)->Result<std::process::Output,String>{
    use tokio::io::AsyncWriteExt;
    if let Some(bytes)=bytes {
        let mut stdin=child.stdin.take().ok_or("저장된 역할 복원 입력을 연결하지 못했습니다.")?;
        stdin.write_all(&bytes).await.map_err(|_| "저장된 역할 복원 입력을 전달하지 못했습니다.".to_string())?;
        stdin.shutdown().await.map_err(|_| "저장된 역할 복원 입력 전달을 마치지 못했습니다.".to_string())?;
    }
    child.wait_with_output().await.map_err(|_| "업데이트 저장 또는 복원 명령 결과를 읽지 못했습니다.".to_string())
}

fn preserved_files(settings_file:&Path) -> Result<Value,String> {
    let mut files=serde_json::Map::new();
    for (name,path) in [
        ("departments",cys::home_dir().join(".cys/depts.json")),
        ("naming",cys::home_dir().join(".cys/naming.json")),
        ("agent_registry",cys::pack::pack_dir().join("agents.json")),
        ("ui_settings",settings_file.to_path_buf()),
    ] {
        let hash=match std::fs::read_to_string(path) {
            Ok(text)=>Some(cys::pack::content_hash_pub(&text)),
            Err(e) if e.kind()==std::io::ErrorKind::NotFound=>None,
            Err(e)=>return Err(format!("사용자 설정 보존 확인 실패: {e}")),
        };
        files.insert(name.into(),json!(hash));
    }
    Ok(Value::Object(files))
}

// Metadata-only capture (privacy): raw surface.list rows carry full cmd argv, env flags and usage.
// Both capture branches (live owner / zero owner) persist only this whitelisted view.
fn live_capture_surfaces(raw:&Value)->Result<(Vec<Value>,usize),String>{
    let live:Vec<Value>=raw.as_array().ok_or("좌석 목록이 올바르지 않습니다.")?
        .iter().filter(|s|s["exited"]==false).cloned().collect();
    let seats=cys::update_snapshot::metadata_only(&Value::Array(live.clone()))?;
    // A custom command that is not a bare known shell is not captured: count it so the notice stays visible.
    let seats:Vec<Value>=seats.into_iter().map(|mut s|{s["exited"]=json!(false);s}).collect();
    let dropped=live.iter().zip(&seats).filter(|(r,s)|!r["cmd"].is_null()&&s["cmd"].is_null()).count();
    Ok((seats,dropped))
}
// Daemon state-dir contract: Unix = socket parent, Windows primary = %LOCALAPPDATA%/cys. Disk field `entries` -> RPC field `saved`.
fn state_dir_for(socket:&Path)->Option<PathBuf>{
    #[cfg(windows)]{let _=socket;std::env::var_os("LOCALAPPDATA").map(|b|PathBuf::from(b).join("cys"))}
    #[cfg(not(windows))]{socket.parent().map(Path::to_path_buf)}
}
fn topology_saved_from(dir:&Path)->Option<Value>{
    let value:Value=serde_json::from_str(&std::fs::read_to_string(dir.join("topology.json")).ok()?).ok()?;
    value["entries"].is_array().then(||json!({"saved":value["entries"]}))
}
fn zero_owner_capture(load:cys::update_snapshot::Load)->(Vec<Value>,Option<Value>,Option<String>){
    match load {
        cys::update_snapshot::Load::Ready(p)=>match cys::update_snapshot::metadata_only(&Value::Array(p.seats)) {
            Ok(seats)=>{let snap=json!({"reference":p.reference,"drain_status":p.drain_status});(seats,Some(snap),None)}
            Err(e)=>(vec![],None,Some(e)),
        },
        cys::update_snapshot::Load::Fallback(n)=>(vec![],None,Some(n)),
    }
}

fn ready_agent(original:&Value,seat:&Value)->Result<Option<u64>,String>{
    if seat["role"]!=original["role"] || seat["agent"]!=original["agent"] || seat["exited"]!=false {
        return Err("복원할 실행 좌석의 역할과 에이전트 신원이 다릅니다.".into());
    }
    let sid=seat["surface_id"].as_u64().filter(|n|*n>0).ok_or("복원한 실행 좌석 번호가 없습니다.")?;
    if seat["agent_alive"]!=true || seat["launch_complete"]!=true {return Ok(None);}
    let cwd=original["live_cwd"].as_str().filter(|s|!s.is_empty()).or_else(||original["cwd"].as_str());
    if cwd!=seat["cwd"].as_str()
        || original["model"].as_str().is_some_and(|m|seat["model"].as_str()!=Some(m))
        || original["claude_config_dir"].as_str().is_some_and(|c|seat["claude_config_dir"].as_str()!=Some(c)) {
        return Err("복원한 실행 좌석의 모델·작업 폴더·계정이 저장한 설정과 다릅니다.".into());
    }
    Ok(Some(sid))
}

impl Backend for Native {
    async fn inspect(&mut self)->Result<Observation,String> {
        if rename_handover::read_marker()?.is_some() { return Err("이전 업데이트 복구 기록이 남아 있습니다. 기존 프로세스를 유지하고 복구 안내를 확인하세요.".into()); }
        self.platform=platform::capture(&self.cli,&self.daemon)?;
        Ok(Observation{owner:self.owner().await?,legacy_registration:self.platform["legacy_registration"]==true,
            canonical_registration:self.platform["canonical_registration"]==true,uncertain:false})
    }
    async fn capture(&mut self,before:&Observation)->Result<Value,String> {
        self.platform["checkpoint_owner"]=json!(before.owner.as_ref().map(|o|&o.identity));
        let mut snapshot=json!({"platform":self.platform,"preserved":preserved_files(&self.settings_file)?,"surfaces":[],"topology":null});
        if let Some(owner)=&before.owner {
            self.same_owner(owner).await?;
            let surfaces=request(&self.socket,"surface.list",json!({})).await?;
            let raw=&surfaces["surfaces"];
            // Raw rows (full cmd argv, env flags, usage) stay in memory for the checks below; only metadata is persisted.
            let (seats,dropped)=live_capture_surfaces(raw)?;
            let raw_live:Vec<Value>=raw.as_array().unwrap().iter().filter(|s|s["exited"]==false).cloned().collect();
            if dropped>0 {snapshot["notice"]=json!("사용자 지정 명령으로 실행 중이던 좌석은 일반 셸로 복원됩니다.");}
            snapshot["surfaces"]=json!(seats);
            snapshot["topology"]=request(&self.socket,"system.topology",json!({})).await?;
            for surface in &raw_live {
                if !surface["agent"].is_null() && surface["launch_complete"]!=true {
                    return Err("기동 중인 에이전트 좌석이 있어 완료를 기다린 뒤 업데이트를 다시 시도해 주세요.".into());
                }
                if surface["agent_alive"]==true && !surface["agent"].is_null() && (surface["role"].as_str().is_none()
                    || !snapshot["topology"]["saved"].as_array().ok_or("역할 복원 목록이 없습니다.")?
                        .iter().any(|entry|entry["role"]==surface["role"]&&entry["agent"]==surface["agent"])) {
                    return Err("복원 기록이 없는 실행 좌석이 있어 업데이트를 보류합니다.".into());
                }
            }
            self.same_owner(owner).await?;
        } else {
            let (seats,layout,notice)=zero_owner_capture(cys::update_snapshot::load(&self.socket));
            if !seats.is_empty() {
                // Captured BEFORE any replacement owner is activated; unreadable source stays null (fail closed).
                snapshot["topology"]=state_dir_for(&self.socket).and_then(|d|topology_saved_from(&d)).unwrap_or(Value::Null);
            }
            snapshot["surfaces"]=json!(seats);
            snapshot["layout_snapshot"]=layout.unwrap_or(Value::Null);
            snapshot["notice"]=notice.map(Value::from).unwrap_or(Value::Null);
        }
        Ok(snapshot)
    }
    async fn record(&mut self,phase:&str,before:&Observation,snapshot:&Value)->Result<(),String> {
        let previous=std::mem::take(&mut self.marker);
        self.marker=json!({"rename_handover":{"phase":phase,"socket":self.socket,
            "old_identity":before.owner.as_ref().map(|o|&o.identity),"before_owner":before.owner,
            "snapshot":snapshot,"revision":self.platform["attempt_id"],"new_owner":self.new_owner,"surface_map":self.surface_map}});
        self.marker["rename_handover"]["snapshot"]["platform"]=self.platform.clone();
        carry_rollback_receipt(&previous,&mut self.marker);
        if phase=="prepared" { rename_handover::prepare_marker(&self.socket,&self.marker,before.owner.as_ref().map(|o|&o.identity)) }
        else { rename_handover::write_marker(Some(&self.marker)) }
    }
    async fn prepare(&mut self,_:&Value)->Result<(),String>{self.platform_change(platform::prepare)}
    async fn checkpoint(&mut self,before:&Observation,snapshot:&mut Value)->Result<(),String>{
        let owner=before.owner.as_ref().ok_or("저장할 서비스가 없습니다.")?;
        self.same_owner(owner).await?;
        let output=self.cli(&["drain","--verify","--timeout","60"]).await?;
        let report:Value=serde_json::from_slice(&output.stdout).map_err(|_|"저장 검증 응답을 확인하지 못했습니다.")?;
        crate::rename_checkpoint::validate(&report,&self.socket,&snapshot["surfaces"])?;
        self.same_owner(owner).await?;
        let current=request(&self.socket,"surface.list",json!({})).await?;
        let ids=|items:&Value|items.as_array().map(|a|a.iter().filter(|s|s["exited"]!=true).map(|s|s["surface_id"].clone()).collect::<Vec<_>>());
        if ids(&current["surfaces"])!=ids(&snapshot["surfaces"]) { return Err("저장 확인 중 좌석 목록이 바뀌어 업데이트를 보류합니다.".into()); }
        snapshot["checkpoint"]=report;
        Ok(())
    }
    async fn disable_legacy(&mut self,_:&Value)->Result<(),String>{self.platform_change(platform::disable_legacy)}
    async fn stop_old(&mut self,owner:&Owner)->Result<(),String>{
        // bootout can already have stopped the verified launchd owner.
        if rename_handover::identity_alive(&owner.identity)? {
            self.same_owner(owner).await?;
            #[cfg(windows)] {
                // Preserve the shipped Windows cleanup of scoped work before TerminateProcess.
                let ledger=request(&self.socket,"ledger.list",json!({})).await?;
                for entry in ledger["entries"].as_array().ok_or("종료 전 작업 원장을 확인하지 못했습니다.")? {
                    if entry["scoped"]==true {
                        let pid=entry["pid"].as_u64().filter(|n|*n>1).ok_or("종료할 작업 번호를 확인하지 못했습니다.")?;
                        self.same_owner(owner).await?;
                        request(&self.socket,"ledger.kill",json!({"pid":pid})).await?;
                    }
                }
                self.same_owner(owner).await?;
            }
            rename_handover::terminate_identity(&owner.identity)?;
        }
        Ok(())
    }
    async fn wait_old_exit(&mut self,owner:&Owner)->Result<(),String>{self.wait_gone(owner).await?;self.platform["old_owner_stopped"]=json!(true);Ok(())}
    async fn activate(&mut self)->Result<(),String>{
        if self.owner().await?.is_some(){return Err("소켓 소유자가 남아 있어 새 서비스를 시작하지 않습니다.".into());}
        self.platform["old_owner_stopped"]=json!(true);
        self.platform_change(platform::activate)
    }
    async fn await_new(&mut self)->Result<Owner,String>{
        for _ in 0..600 {
            if let Some(owner)=self.owner().await? {
                if !owner.canonical||owner.version!=env!("CARGO_PKG_VERSION"){return Err("새 서비스의 실행 경로와 버전을 확인하지 못했습니다.".into());}
                self.new_owner=Some(owner.clone());return Ok(owner);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err("새 서비스 시작 권한을 기다리는 중입니다. 시스템 권한 안내를 확인해 주세요.".into())
    }
    async fn restore(&mut self,snapshot:&Value,owner:&Owner)->Result<(),String>{
        self.same_owner(owner).await?;
        self.cli(&["init-pack","--no-install-hook"]).await?;
        for original in snapshot["surfaces"].as_array().ok_or("저장한 좌석 목록이 없습니다.")? {
            let old_sid=original["surface_id"].as_u64().filter(|n|*n>0).ok_or("저장된 좌석 번호가 없습니다.")?;
            let old_key=old_sid.to_string();
            let cwd=original["live_cwd"].as_str().filter(|s|!s.is_empty()).or_else(||original["cwd"].as_str())
                .ok_or("저장된 좌석의 작업 폴더를 확인하지 못했습니다.")?;
            let was_live_agent=original["agent_alive"]==true && original["agent"].as_str().is_some_and(|s|!s.is_empty());
            let current=request(&self.socket,"surface.list",json!({})).await?;
            let live=current["surfaces"].as_array().ok_or("복원된 좌석 목록이 없습니다.")?;
            let sid=if let Some(saved)=self.surface_map.get(&old_key) {
                let sid=saved.as_u64().filter(|n|*n>0).ok_or("좌석 복원 번호 기록이 잘못됐습니다.")?;
                let seat=live.iter().find(|s|s["surface_id"].as_u64()==Some(sid)&&s["exited"]==false)
                    .ok_or("이미 복원한 좌석이 없어 재실행하지 않습니다. 복원 기록을 유지합니다.")?;
                if seat["role"]!=original["role"] || (was_live_agent && seat["agent"]!=original["agent"])
                    || (!was_live_agent && !seat["agent"].is_null()) {
                    return Err("이미 복원한 좌석의 신원이 달라 재실행하지 않습니다.".into());
                }
                if was_live_agent && ready_agent(original,seat)?.is_none(){return Err("이미 복원한 에이전트의 실행을 확인하지 못해 재실행하지 않습니다. 복원 기록을 유지합니다.".into());}
                sid
            } else if was_live_agent {
                let role=original["role"].as_str().filter(|s|!s.is_empty()).ok_or("실행 좌석의 역할이 없습니다.")?;
                let mut args=vec!["restore","--saved-stdin","--include-master","--only-role",role,"--cwd",cwd];
                if let Some(model)=original["model"].as_str().filter(|s|!s.is_empty()){args.extend(["--model",model]);}
                let saved=&snapshot["topology"]["saved"];
                if !saved.is_array(){return Err("저장된 역할 복원 원본이 없습니다. 기존 기록을 유지합니다.".into());}
                self.cli_input(&args,Some(saved)).await?;
                let mut restored=None;
                for _ in 0..600 {
                    self.same_owner(owner).await?;
                    let state=request(&self.socket,"surface.list",json!({})).await?;
                    let seats=state["surfaces"].as_array().ok_or("복원된 실행 좌석 목록이 없습니다.")?;
                    let matches:Vec<_>=seats.iter().filter(|s|s["role"]==original["role"]&&s["exited"]==false).collect();
                    if matches.len()>1{return Err("같은 역할의 실행 좌석이 여러 개여서 복원을 보류합니다.".into());}
                    if let Some(seat)=matches.first() {
                        if let Some(sid)=ready_agent(original,seat)? {restored=Some(sid);break;}
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                restored.ok_or("실행 좌석의 기동 완료를 확인하지 못했습니다. 복원 기록을 유지합니다.")?
            } else {
                let created=request(&self.socket,"surface.create",json!({"cwd":cwd,"cmd":original["cmd"],
                    "title":original["title"],"role":original["role"],"claude_config_dir":original["claude_config_dir"],
                    "idempotency_key":format!("wave-rename:{}:{old_sid}",self.platform["attempt_id"].as_str().ok_or("이관 식별자가 없습니다.")?)})).await?;
                created["surface_id"].as_u64().filter(|n|*n>0).ok_or("일반 좌석 복원을 확인하지 못했습니다.")?
            };
            // Persist immediately; reserved live-seat keys cover a crash before this write.
            self.surface_map.insert(old_key,json!(sid));
            self.persist_surface_map()?;
            if let Some(title)=original["title"].as_str(){request(&self.socket,"surface.rename",json!({"surface_id":sid,"title":title})).await?;}
            if !original["label"].is_null(){request(&self.socket,"surface.set_label",json!({"surface_id":sid,"label":original["label"]})).await?;}
            self.same_owner(owner).await?;
        }
        if preserved_files(&self.settings_file)?!=snapshot["preserved"]{return Err("사용자 설정 보존 대조가 일치하지 않습니다. 복원 기록을 유지합니다.".into());}
        self.same_owner(owner).await
    }
    async fn finish(&mut self)->Result<(),String>{
        self.marker["rename_handover"]["phase"]=json!("awaiting-layout");
        self.marker["rename_handover"]["surface_map"]=json!(self.surface_map);
        rename_handover::write_marker(Some(&self.marker))
    }
    async fn rollback(&mut self,snapshot:&Value,before:&Observation)->Result<(),String>{
        let mut original_serving=false;
        if let Some(current)=self.owner().await? {
            if before.owner.as_ref().is_some_and(|old|old.identity==current.identity) {
                original_serving=true;
                self.platform["new_owner_stopped"]=json!(true);
            } else if self.new_owner.as_ref().is_some_and(|new|new.identity==current.identity) {
                // Never discard seats already restored or created while recovery was running.
                let seats=request(&self.socket,"surface.list",json!({})).await?;
                if seats["surfaces"].as_array().is_none_or(|a|a.iter().any(|s|s["exited"]==false)) {
                    return Err("새 서비스에 실행 좌석이 있어 자동 종료하지 않습니다. 복원 기록을 유지합니다.".into());
                }
                rename_handover::terminate_identity(&current.identity)?;self.wait_gone(&current).await?;
                self.platform["new_owner_stopped"]=json!(true);
            } else {return Err("소유권이 불확실해 자동 복구를 중단합니다. 기존 프로세스는 종료하지 않습니다.".into());}
        } else {self.platform["new_owner_stopped"]=json!(true);}
        let recovering=before.owner.is_some() && !original_serving;
        if recovering {
            // The restored old registration may start the canonical image; the startup guard only allows `starting-new`.
            self.record("starting-new",before,snapshot).await?;
            self.marker["rename_handover"]["rollback_recovery_pending"]=json!(true);
            rename_handover::write_marker(Some(&self.marker))?;
        }
        let platform_result=self.platform_change(platform::rollback);
        if let Err(error)=&platform_result {
            // Keep the OS error as durable data, not only a transient return string.
            self.marker["rename_handover"]["rollback_diagnostic"]=json!(error);
            rename_handover::write_marker(Some(&self.marker))?;
        }
        let links_ok=!cfg!(target_os="macos") || self.platform["links_rolled_back"]==true;
        match rollback_plan(before.owner.is_some(),original_serving,platform_result.is_ok(),
            self.platform["old_registration_restored"]==true,links_ok,snapshot["checkpoint"]["all_saved"]==true) {
            RollbackPlan::Clear => { platform_result?; rename_handover::write_marker(None) }
            RollbackPlan::KeepMarker => Err(platform_result.err().unwrap_or_else(||
                "기존 서비스 복구 조건이 확인되지 않아 자동 시작하지 않습니다. 복원 기록을 유지합니다.".into())),
            RollbackPlan::RecoverSeats => {
                if let Err(error)=self.recover_seats(snapshot,before).await {
                    return Err(match platform_result {
                        Err(os)=>format!("{os}\n기존 좌석 복구도 완료하지 못했습니다: {error}"),
                        Ok(())=>error,
                    });
                }
                // Seats are back and the layout acknowledgement still has to complete; an OS error stays an error.
                platform_result.map_err(|os|format!("기존 좌석은 복구했지만 등록 정리는 끝나지 않았습니다: {os}"))
            }
        }
    }
}

impl Native {
    /// Resume an interrupted rollback recovery from its durable receipt (review #3). Never forward-activates.
    async fn resume_rollback_recovery(&mut self,snapshot:&Value,before:&Observation,owner_present:bool)->Result<Outcome,String>{
        let diagnostic=self.marker["rename_handover"]["rollback_diagnostic"].as_str().map(str::to_string);
        let original_alive=match before.owner.as_ref() {
            Some(old)=>rename_handover::identity_alive(&old.identity)?,
            None=>false,
        };
        let links_ok=!cfg!(target_os="macos") || self.platform["links_rolled_back"]==true;
        let plan=resume_plan(&self.platform,snapshot["checkpoint"]["all_saved"]==true,before.owner.is_some(),
            original_alive,owner_present,links_ok);
        if plan==ResumePlan::Refuse {
            let refusal="기존 등록 복구 영수증이 확인되지 않아 서비스를 시작하지 않습니다. 복원 기록을 유지합니다.";
            return Err(diagnostic.map_or(refusal.to_string(),|d|format!("{d}\n{refusal}")));
        }
        if plan==ResumePlan::StartRestored { self.platform_change(platform::start_restored_owner)?; }
        self.recover_seats(snapshot,before).await?;
        // Seats are back; the layout acknowledgement completes it. Never an unqualified update success.
        Err(match diagnostic {
            Some(d)=>format!("기존 좌석은 복구했지만 등록 정리는 끝나지 않았습니다: {d}"),
            None=>"업데이트를 되돌리고 기존 좌석을 복구했습니다. 앱을 다시 열면 배치를 저장합니다.".into(),
        })
    }
}

pub(super) async fn run(settings_file:PathBuf)->Result<Outcome,String>{
    let Some(marker)=rename_handover::read_marker()? else { return perform(&mut Native::new(settings_file.clone())?).await; };
    let record=&marker["rename_handover"];
    let phase=record["phase"].as_str().ok_or("이전 업데이트 단계가 없습니다.")?;
    let mut native=Native::new(settings_file.clone())?;
    if record["socket"].as_str()!=native.socket.to_str(){return Err("이전 업데이트 기록의 소켓이 달라 자동 복구를 보류합니다.".into());}
    native.marker=marker.clone();
    native.platform=record["snapshot"]["platform"].clone();
    native.new_owner=serde_json::from_value(record["new_owner"].clone()).map_err(|_|"새 프로세스 복구 기록이 올바르지 않습니다.")?;
    native.surface_map=record["surface_map"].as_object().cloned().unwrap_or_default();
    if !native.surface_map.is_empty() && native.new_owner.is_none(){return Err("좌석 복원 기록의 서비스 신원이 없어 재실행하지 않습니다.".into());}
    let original:Option<Owner>=serde_json::from_value(record["before_owner"].clone()).map_err(|_|"기존 프로세스 복구 기록이 올바르지 않습니다.")?;
    let snapshot=record["snapshot"].clone();
    let before=Observation{owner:original.clone(),legacy_registration:native.platform["legacy_registration"]==true,
        canonical_registration:native.platform["canonical_registration"]==true,uncertain:false};
    let current=native.owner().await?;
    if phase=="awaiting-layout" {
        if current.as_ref().map(|o|&o.identity)!=native.new_owner.as_ref().map(|o|&o.identity) || current.is_none() {
            return Err("배치 저장 전에 서비스가 다시 시작됐습니다. 이전 배치를 유지하고 복원 안내를 확인하세요.".into());
        }
        return Ok(Outcome::AlreadyReady);
    }
    if record["rollback_recovery_pending"]==true {
        return native.resume_rollback_recovery(&snapshot,&before,current.is_some()).await;
    }
    if (phase=="prepared" && original.is_none() && current.is_none())
        || current.as_ref().is_some_and(|owner|original.as_ref().is_some_and(|old|owner.identity==old.identity)) {
        native.rollback(&snapshot,&before).await?;
        return perform(&mut Native::new(settings_file.clone())?).await;
    }
    if !matches!(phase,"stopping-old"|"starting-new"|"restoring")
        || snapshot["checkpoint"]["all_saved"]!=true && original.is_some() {
        return Err("프로세스 교대 전의 저장 확인 기록이 없어 자동 복구를 보류합니다. 이전 기록을 유지합니다.".into());
    }
    if let Some(old)=original.as_ref() {
        if rename_handover::identity_alive(&old.identity)? { return Err("기존 프로세스 신원이 남아 있어 새 서비스를 시작하지 않습니다.".into()); }
    }
    let owner=if let Some(owner)=current {
        if !owner.canonical||owner.version!=env!("CARGO_PKG_VERSION"){return Err("복구할 서비스의 실행 경로와 버전이 다릅니다.".into());}
        if native.new_owner.as_ref().is_some_and(|previous|previous.identity!=owner.identity){native.surface_map.clear();}
        native.new_owner=Some(owner.clone());owner
    } else {
        native.platform["old_owner_stopped"]=json!(true);
        native.record("starting-new",&before,&snapshot).await?;
        native.activate().await?;
        native.await_new().await?
    };
    native.record("restoring",&before,&snapshot).await?;
    native.restore(&snapshot,&owner).await?;
    native.finish().await?;
    Ok(Outcome::Migrated)
}

pub(super) async fn validate_layout_owner(marker:&Value,settings_file:PathBuf)->Result<(),String>{
    let native=Native::new(settings_file.clone())?;
    if marker["rename_handover"]["socket"].as_str()!=native.socket.to_str(){return Err("복원 기록과 현재 소켓이 달라 배치 저장을 보류합니다.".into());}
    let owner=native.owner().await?.ok_or("좌석 배치 저장 전에 서비스가 종료됐습니다. 복원 기록을 유지합니다.")?;
    if !owner.canonical || owner.version!=env!("CARGO_PKG_VERSION"){return Err("좌석 배치의 서비스 버전과 실행 파일을 확인하지 못했습니다.".into());}
    let live=request(&native.socket,"surface.list",json!({})).await?;
    crate::rename_layout::validate(marker,&owner.identity,&live)?;
    native.same_owner(&owner).await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn poisoned()->Value{json!([
        {"surface_id":3,"role":"worker","agent":"claude","model":"m","cwd":"/w","live_cwd":"/w","title":"t","label":null,
         "claude_config_dir":"/acct","agent_alive":true,"launch_complete":true,"exited":false,
         "cmd":["claude","--api-key=SECRET-ARG","--resume","SECRET-SESSION"],"env_injected":true,"pid":42,
         "usage":{"tokens":"SECRET-USAGE"},"token":"SECRET-UNKNOWN","scrollback":"SECRET-SCROLL"},
        {"surface_id":4,"role":null,"agent":null,"cwd":"/w","title":"sh","exited":false,"cmd":["/bin/zsh"],"extra":"SECRET-EXTRA"},
        {"surface_id":5,"role":null,"agent":null,"cwd":"/w","title":"x","exited":false,"cmd":["python3","-c","SECRET-CUSTOM"]},
        {"surface_id":6,"exited":true,"cmd":["bash","SECRET-EXITED"]}])}
    fn assert_metadata_only(seats:&[Value]){
        let text=serde_json::to_string(seats).unwrap();
        assert!(!text.contains("SECRET"),"leaked: {text}");
        let allowed=["surface_id","role","agent","cwd","live_cwd","title","label","model","agent_alive","launch_complete","cmd","exited"];
        for seat in seats { for key in seat.as_object().unwrap().keys(){assert!(allowed.contains(&key.as_str()),"unknown field {key}");} }
        assert!(seats.iter().all(|s|s["cmd"].is_null()||s["cmd"].as_array().is_some_and(|a|a.len()==1)));
    }
    #[test]
    fn live_owner_capture_persists_metadata_only_no_argv_or_unknown_fields(){
        let (seats,dropped)=live_capture_surfaces(&poisoned()).unwrap();
        assert_metadata_only(&seats);
        assert_eq!(seats.len(),3,"exited seat must not be captured");
        assert_eq!(seats[1]["cmd"],json!(["/bin/zsh"]),"plain shell kept");
        assert!(seats[0]["cmd"].is_null()&&seats[2]["cmd"].is_null(),"custom argv dropped");
        assert_eq!(dropped,2,"dropped custom commands are counted so the notice stays visible");
        assert!(seats[0].get("claude_config_dir").is_none(),"account path is not persisted (stage-1 contract)");
    }
    #[test]
    fn zero_owner_capture_persists_metadata_only_even_from_a_tampered_snapshot(){
        let ident=cys::rename_handover::ProcessIdentity{pid:1,start_time:1,executable:"/x".into()};
        let load=cys::update_snapshot::Load::Ready(cys::update_snapshot::Pending{
            reference:cys::update_snapshot::Reference{path:"/x".into(),id:"0".repeat(32),digest:"0".repeat(64)},
            daemon:ident,seats:poisoned().as_array().unwrap().clone(),drain_status:None});
        let (seats,snapshot,_notice)=zero_owner_capture(load);
        assert_metadata_only(&seats);
        assert!(snapshot.is_some(),"layout_snapshot reference/drain_status carried");
    }
    #[test]
    fn zero_owner_fallback_has_no_seats_and_a_visible_notice(){
        let (seats,snapshot,notice)=zero_owner_capture(cys::update_snapshot::Load::Fallback("안내".into()));
        assert!(seats.is_empty()&&snapshot.is_none()&&notice.as_deref()==Some("안내"));
    }
    #[test]
    fn zero_owner_topology_comes_from_disk_entries_and_never_a_guess(){
        let dir=transport_root().join("topology-case");
        let _=std::fs::remove_dir_all(&dir);std::fs::create_dir_all(&dir).unwrap();
        assert!(topology_saved_from(&dir).is_none(),"missing file is not an empty list");
        std::fs::write(dir.join("topology.json"),"{not json").unwrap();
        assert!(topology_saved_from(&dir).is_none(),"corrupt file is not an empty list");
        std::fs::write(dir.join("topology.json"),r#"{"entries":[{"role":"worker","agent":"claude"}]}"#).unwrap();
        assert_eq!(topology_saved_from(&dir).unwrap()["saved"][0]["role"],"worker");
        let _=std::fs::remove_dir_all(&dir);
    }
    fn original()->Value{json!({"role":"worker","agent":"codex","model":"model-1","cwd":"/old","live_cwd":"/active","claude_config_dir":"/account"})}
    fn seat()->Value{json!({"surface_id":7,"role":"worker","agent":"codex","model":"model-1","cwd":"/active","claude_config_dir":"/account","exited":false,"agent_alive":true,"launch_complete":true})}
    #[test]
    fn previous_bundle_daemon_accepts_only_exact_shipped_and_nsis_swap_names(){
        let base=Path::new("/app/Wave");
        for ok in ["cysd","cysd.exe","cysd.prev.exe","cysd.prev2.exe","cysd.prev3.exe"] {
            assert!(is_previous_bundle_daemon(&base.join(ok),base),"{ok}");
        }
        for bad in ["cysd.prev4.exe","cysd.prev","cysd.prev.exe.bak","cysd.exe.prev123","cysd.exe.previous","xcysd.exe","cysd2.exe","CYSD.EXE","cys.exe","waved.exe",""] {
            assert!(!is_previous_bundle_daemon(&base.join(bad),base),"{bad}");
        }
        assert!(!is_previous_bundle_daemon(Path::new("/other/Wave/cysd.prev.exe"),base),"foreign dir");
        assert!(!is_previous_bundle_daemon(Path::new("/app/Wave/sub/cysd.prev.exe"),base),"subdir");
        assert!(!is_previous_bundle_daemon(Path::new("/app/Wave/../Wave2/cysd.prev.exe"),base),"traversal");
        assert!(!is_previous_bundle_daemon(Path::new("/app/Wave/sub/../cysd.prev.exe"),base),"traversal2");
        assert!(!is_previous_bundle_daemon(Path::new("cysd.prev.exe"),base),"relative");
        assert!(!is_previous_bundle_daemon(Path::new("/app/Wave"),Path::new("/app/Wave")),"dir itself");
    }
    #[test]
    fn rollback_plan_recovers_seats_before_the_marker_is_ever_cleared(){
        use RollbackPlan::*;
        // (had_owner, original_serving, platform_ok, registration_restored, links_ok, all_saved) -> plan
        let rows=[
            ((false,false,true ,false,false,false),Clear),      // CLI-autostart original, clean rollback
            ((false,false,false,true ,true ,true ),KeepMarker), // no owner to recover but OS error: keep evidence
            ((true ,true ,true ,false,false,false),Clear),      // original still serving: nothing to restart
            ((true ,true ,false,true ,true ,true ),KeepMarker), // original still serving + OS error: no restart, keep
            ((true ,false,true ,true ,true ,true ),RecoverSeats),  // clean rollback, owner was stopped
            ((true ,false,false,true ,true ,true ),RecoverSeats),  // OS Err AFTER durable restore receipt (review defect 1)
            ((true ,false,false,true ,false,true ),KeepMarker), // registration restored but Mac links not yet: unsafe start
            ((true ,false,false,false,true ,true ),KeepMarker), // Err before a safe receipt: no start, no clearing
            ((true ,false,true ,true ,true ,false),KeepMarker), // no verified checkpoint: never start seats blindly
            ((true ,false,true ,false,false,true ),RecoverSeats),  // adapter Ok is itself the receipt
        ];
        for ((a,b,c,d,e,f),want) in rows {
            assert_eq!(rollback_plan(a,b,c,d,e,f),want,"{a} {b} {c} {d} {e} {f}");
        }
    }
    #[test]
    fn restart_with_rollback_receipt_routes_to_recovery_never_forward_activation(){
        use ResumePlan::*;
        let safe=json!({"old_registration_restored":true,"links_rolled_back":true});
        // (platform, all_saved, had_owner, original_alive, owner_present, links_ok) -> plan
        assert_eq!(resume_plan(&safe,true,true,false,false,true),StartRestored);   // restored legacy job must start the owner
        assert_eq!(resume_plan(&safe,true,true,false,true ,true),Recover);         // owner already back (alias -> canonical)
        let requested=json!({"old_registration_restored":true,"restored_owner_start_requested":true});
        assert_eq!(resume_plan(&requested,true,true,false,false,true),Recover);    // ONE start request only; then just poll
        assert_eq!(resume_plan(&safe,true,true,true ,false,true),Refuse);          // original identity still alive
        assert_eq!(resume_plan(&json!({}),true,true,false,false,true),Refuse);     // Err before a safe receipt: no start
        assert_eq!(resume_plan(&safe,true,true,false,false,false),Refuse);         // Mac links not rolled back: unsafe start
        assert_eq!(resume_plan(&safe,false,true,false,false,true),Refuse);         // no verified checkpoint
        assert_eq!(resume_plan(&safe,true,false,false,false,true),Refuse);         // no original owner: never pretend a registration
        assert_eq!(resume_plan(&json!({"rollback_succeeded":true}),true,true,false,true,true),Recover);
    }
    #[test]
    fn rollback_receipt_and_diagnostic_survive_every_marker_rebuild(){
        let previous=json!({"rename_handover":{"phase":"starting-new","rollback_recovery_pending":true,
            "rollback_diagnostic":"알 수 없는 새 등록 파일을 보존했습니다."}});
        let mut next=json!({"rename_handover":{"phase":"restoring"}});
        carry_rollback_receipt(&previous,&mut next);
        assert_eq!(next["rename_handover"]["rollback_recovery_pending"],true);
        assert_eq!(next["rename_handover"]["rollback_diagnostic"],"알 수 없는 새 등록 파일을 보존했습니다.");
        assert_eq!(next["rename_handover"]["phase"],"restoring");
        let mut fresh=json!({"rename_handover":{"phase":"prepared"}});
        carry_rollback_receipt(&Value::Null,&mut fresh);
        assert!(fresh["rename_handover"].get("rollback_recovery_pending").is_none());
        assert!(fresh["rename_handover"].get("rollback_diagnostic").is_none());
    }
    #[test]
    fn recovered_layout_keeps_unresolved_registration_visible_without_duplicates(){
        let snapshot=json!({"notice":"사용자 지정 명령으로 실행 중이던 좌석은 일반 셸로 복원됩니다.","checkpoint":{"all_saved":true}});
        let once=with_rollback_notice(&snapshot,Some("등록 정리 실패"));
        let text=once["notice"].as_str().unwrap();
        assert!(text.contains("일반 셸") && text.contains("등록 정리 실패") && text.contains("끝나지 않았습니다"));
        assert_eq!(with_rollback_notice(&once,Some("등록 정리 실패")),once);
        assert_eq!(once["checkpoint"],snapshot["checkpoint"]);
        assert_eq!(with_rollback_notice(&snapshot,None),snapshot);
        let empty=with_rollback_notice(&json!({"notice":null}),Some("x"));
        assert!(empty["notice"].as_str().unwrap().contains("x"));
    }
    #[cfg(unix)]
    fn transport_root()->PathBuf{
        use std::os::unix::fs::MetadataExt;
        let path=PathBuf::from(std::env::var_os("WAVE_TEST_ROOT").expect("explicit private test root required"));
        assert!(path.is_absolute()&&path.starts_with("/private/tmp")&&path.components().all(|c|!matches!(c,std::path::Component::ParentDir)));
        let mut prefix=PathBuf::new();
        for part in path.components(){prefix.push(part);assert!(!prefix.symlink_metadata().unwrap().file_type().is_symlink());}
        let metadata=path.symlink_metadata().unwrap();
        assert!(metadata.is_dir()&&metadata.uid()==unsafe{libc::geteuid()}&&metadata.mode()&0o077==0);
        assert_eq!(path.canonicalize().unwrap(),path);
        path
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn private_cli_input_reaches_eof_without_argument_payload(){
        let mut command=tokio::process::Command::new("/bin/cat");
        command.current_dir(transport_root()).stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
        let child=command.spawn().unwrap();
        let payload=serde_json::to_vec(&json!([{"role":"fixture","agent":null,"cwd":"/fixture"}])).unwrap();
        let output=tokio::time::timeout(Duration::from_secs(2),finish_cli_input(child,Some(payload.clone()))).await.unwrap().unwrap();
        assert!(output.status.success());assert_eq!(output.stdout,payload);assert!(output.stderr.is_empty());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn private_cli_input_timeout_reaps_only_the_owned_child(){
        for payload_size in [0,1024*1024] {
            let mut command=tokio::process::Command::new("/bin/sleep");
            command.arg("60").current_dir(transport_root()).stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
            let child=command.spawn().unwrap();let pid=child.id().unwrap();
            assert!(tokio::time::timeout(Duration::from_millis(20),finish_cli_input(child,Some(vec![0;payload_size]))).await.is_err());
            let mut gone=false;
            for _ in 0..100 {
                if unsafe{libc::kill(pid as i32,0)}==-1&&std::io::Error::last_os_error().raw_os_error()==Some(libc::ESRCH){gone=true;break;}
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(gone,"owned fake child was not reaped after cancellation");
        }
    }
    #[test]
    fn agent_metadata_or_pending_shell_is_not_restore_success(){
        let mut seat=seat();seat["agent_alive"]=json!(false);seat["launch_complete"]=json!(false);
        assert_eq!(ready_agent(&original(),&seat).unwrap(),None);
        seat["agent_alive"]=json!(true);assert_eq!(ready_agent(&original(),&seat).unwrap(),None);
        assert_eq!(ready_agent(&original(),&self::seat()).unwrap(),Some(7));
    }
    #[test]
    fn restored_agent_must_keep_role_model_working_directory_and_account(){
        for (key,value) in [("role",json!("other")),("agent",json!("other")),("model",json!("other")),("cwd",json!("/other")),("claude_config_dir",json!("/other")),("surface_id",json!(0))] {
            let mut changed=seat();changed[key]=value;assert!(ready_agent(&original(),&changed).is_err(),"{key}");
        }
    }
}

#[cfg(target_os="macos")]
pub(super) fn repair_cli_paths()->Result<(),String>{
    let exe=std::env::current_exe().map_err(|e|format!("앱 실행 경로 확인 실패: {e}"))?;
    let directory=exe.parent().ok_or("앱 실행 폴더가 없습니다.")?;
    if matches!(crate::classify_bundle_dir(directory),crate::BundleKind::Translocated|crate::BundleKind::Backup) {
        return Err("정식 Wave Terminal.app을 응용 프로그램 폴더에서 연 뒤 명령 연결을 설치해 주세요.".into());
    }
    let cli=directory.join("wave");let daemon=directory.join("waved");
    for file in [&cli,&daemon] {
        let metadata=file.symlink_metadata().map_err(|e|format!("번들 실행 파일 확인 실패: {e}"))?;
        if !metadata.is_file()||metadata.file_type().is_symlink(){return Err("정식 번들의 wave/waved 실행 파일을 확인하지 못했습니다.".into());}
    }
    platform::repair_cli_links(&cli,&daemon)
}
