//! Private metadata-only handoff for the shipped plain-drain update path.
use crate::rename_handover::ProcessIdentity;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reference { pub path: PathBuf, pub id: String, pub digest: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pending {
    pub reference: Reference,
    pub daemon: ProcessIdentity,
    pub seats: Vec<Value>,
    pub drain_status: Option<i32>,
}
#[derive(Debug)]
pub enum Load { Ready(Pending), Fallback(String) }

const FILE_NAME: &str = ".pending-rename-layout.json";
const MAX_BYTES: usize = 1024 * 1024;
const MAX_SEATS: usize = 256;
const MAX_TEXT: usize = 4096;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum State { Pending, Consumed }

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: u32,
    id: String,
    socket: PathBuf,
    daemon: ProcessIdentity,
    captured_at: u64,
    state: State,
    drain_status: Option<i32>,
    consumed_from: Option<String>,
    seats: Vec<Value>,
    digest: String,
}

fn default_dir() -> Result<PathBuf, String> {
    #[cfg(test)] { Err("단위 시험은 명시적으로 주입한 배치 기록 경로만 사용합니다.".into()) }
    #[cfg(not(test))] {
        let home = crate::home_dir();
        if !home.is_absolute() { return Err("홈 경로를 확인하지 못해 기존 복원을 사용합니다.".into()); }
        Ok(home.join(".cys"))
    }
}

pub fn write(socket: &Path, daemon: &ProcessIdentity, surfaces: &Value) -> Result<Reference, String> {
    write_at(&default_dir()?, socket, daemon, surfaces)
}
pub fn finish_drain(reference: &Reference, status: i32) -> Result<Reference, String> {
    finish_drain_at(&default_dir()?, reference, status)
}
pub fn load(socket: &Path) -> Load {
    match default_dir() { Ok(dir) => load_at(&dir, socket), Err(error) => Load::Fallback(error) }
}
pub fn mark_consumed(reference: &Reference) -> Result<(), String> {
    mark_consumed_at(&default_dir()?, reference)
}

/// Shown on the first GUI start after a drain whose layout snapshot could not be recorded.
pub const DRAIN_FAILED_NOTICE: &str = "창 배치를 되살리지 못해 기본 배치로 열었습니다.";
const FAILED_NAME: &str = ".pending-rename-layout.failed";

/// Plain-drain snapshot start. Never prints and never fails the caller: a failure is recorded as a marker.
pub fn drain_begin(socket: &Path, capture: impl FnOnce() -> Result<(ProcessIdentity, Value), String>) -> Option<Reference> {
    drain_begin_at(&default_dir().ok()?, socket, capture)
}
/// Plain-drain snapshot finish. Never prints and never changes the drain status.
pub fn drain_finish(socket: &Path, reference: Option<Reference>, status: i32) {
    let Some(reference) = reference else { return }; // begin already recorded its failure
    if let Ok(dir) = default_dir() { drain_finish_at(&dir, socket, Some(reference), status) }
}
fn drain_begin_at(dir: &Path, socket: &Path, capture: impl FnOnce() -> Result<(ProcessIdentity, Value), String>) -> Option<Reference> {
    let result = capture().and_then(|(daemon, surfaces)| write_at(dir, socket, &daemon, &surfaces));
    if result.is_err() { let _ = record_failure_at(dir, socket, "begin"); }
    result.ok()
}
fn drain_finish_at(dir: &Path, socket: &Path, reference: Option<Reference>, status: i32) {
    let Some(reference) = reference else { return };
    if finish_drain_at(dir, &reference, status).is_err() { let _ = record_failure_at(dir, socket, "finish"); }
}
/// Metadata-only failure marker (schema, socket, stage, time); the snapshot file itself is never touched.
fn record_failure_at(dir: &Path, socket: &Path, stage: &str) -> Result<(), String> {
    ensure_directory(dir)?;
    let path = dir.join(FAILED_NAME);
    let previous = raw_bytes(dir, &path)?;
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let bytes = serde_json::to_vec(&serde_json::json!({"schema":1,"socket":socket,"stage":stage,"at":at}))
        .map_err(|_| "실패 표식을 만들지 못했습니다.")?;
    publish_bytes(dir, &path, &bytes, previous.as_deref())
}
/// A recorded failure for this socket, or an unreadable marker, forces the fallback (fail closed).
fn recorded_failure(dir: &Path, socket: &Path) -> bool {
    match raw_bytes(dir, &dir.join(FAILED_NAME)) {
        Ok(None) => false,
        Ok(Some(bytes)) => serde_json::from_slice::<Value>(&bytes).ok()
            .and_then(|v| v["socket"].as_str().map(PathBuf::from))
            .is_none_or(|marked| marked == socket),
        Err(_) => true,
    }
}

fn write_at(dir: &Path, socket: &Path, daemon: &ProcessIdentity, surfaces: &Value) -> Result<Reference, String> {
    let mut envelope = Envelope {
        schema:1, id:random_id()?, socket:socket.into(), daemon:daemon.clone(),
        captured_at:std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "배치 기록 시각을 확인하지 못했습니다.")?.as_secs(),
        state:State::Pending, drain_status:None, consumed_from:None, seats:sanitize(surfaces)?, digest:String::new(),
    };
    seal(&mut envelope)?;
    validate(&envelope)?;
    ensure_directory(dir)?;
    let primary = dir.join(FILE_NAME);
    let (path, previous) = match read_owned(dir, &primary) {
        Ok(previous) => (primary, previous),
        Err(_) => {
            let alternative = dir.join(format!("{FILE_NAME}.new"));
            let previous = read_owned(dir, &alternative)
                .map_err(|_| "수정된 원본과 .new 배치 기록을 모두 보존했습니다. 기존 복원을 사용합니다.")?;
            (alternative, previous)
        }
    };
    publish(dir, &path, &envelope, previous.as_ref().map(|(_, bytes)| bytes.as_slice()))?;
    let _ = std::fs::remove_file(dir.join(FAILED_NAME)); // a fresh snapshot supersedes an earlier recorded failure
    Ok(reference(&path, &envelope))
}

fn finish_drain_at(dir: &Path, reference: &Reference, status: i32) -> Result<Reference, String> {
    let (mut envelope, previous) = referenced(dir, reference)?;
    if envelope.state != State::Pending { return Err("이미 적용된 배치 기록의 저장 상태를 바꾸지 않습니다.".into()); }
    envelope.drain_status = Some(status); // Exit status is not proof that every seat was saved.
    seal(&mut envelope)?;
    publish(dir, &reference.path, &envelope, Some(&previous))?;
    Ok(self::reference(&reference.path, &envelope))
}

fn load_at(dir: &Path, socket: &Path) -> Load {
    if recorded_failure(dir, socket) { return Load::Fallback(DRAIN_FAILED_NOTICE.into()); }
    let mut notice = "업데이트 배치 기록이 없어 기존 복원을 사용합니다.";
    for path in [dir.join(format!("{FILE_NAME}.new")), dir.join(FILE_NAME)] {
        match read_owned(dir, &path) {
            Ok(Some((envelope, _))) if envelope.socket == socket && envelope.state == State::Pending => {
                return Load::Ready(Pending { reference:reference(&path, &envelope), daemon:envelope.daemon,
                    seats:envelope.seats, drain_status:envelope.drain_status });
            }
            Ok(Some(_)) => notice = "적용됐거나 다른 서비스의 배치 기록이므로 기존 복원을 사용합니다.",
            Err(_) => notice = "배치 기록이 손상되거나 수정되어 기존 복원을 사용합니다.",
            Ok(None) => (),
        }
    }
    Load::Fallback(notice.into())
}

fn mark_consumed_at(dir: &Path, reference: &Reference) -> Result<(), String> {
    validate_reference(dir, reference)?;
    let (mut envelope, previous) = read_owned(dir, &reference.path)?.ok_or("적용할 배치 기록이 없습니다.")?;
    if envelope.id != reference.id { return Err("다른 배치 기록을 적용 완료로 바꾸지 않습니다.".into()); }
    if envelope.state == State::Consumed {
        return if envelope.consumed_from.as_deref() == Some(&reference.digest) || envelope.digest == reference.digest {
            Ok(())
        } else { Err("적용 완료 기록의 무결성 참조가 다릅니다.".into()) };
    }
    if envelope.digest != reference.digest { return Err("수정된 배치 기록을 적용 완료로 바꾸지 않습니다.".into()); }
    envelope.consumed_from = Some(envelope.digest.clone());
    envelope.state = State::Consumed;
    seal(&mut envelope)?;
    publish(dir, &reference.path, &envelope, Some(&previous))
}

/// Metadata-only view of raw seat rows: whitelisted fields only, custom command argv dropped.
pub fn metadata_only(surfaces: &Value) -> Result<Vec<Value>, String> { sanitize(surfaces) }

fn sanitize(surfaces: &Value) -> Result<Vec<Value>, String> {
    let input = surfaces.as_array().or_else(|| surfaces.get("surfaces").and_then(Value::as_array))
        .ok_or("좌석 배치 목록 형식이 올바르지 않습니다.")?;
    if input.len() > MAX_SEATS { return Err("배치 기록의 좌석 수 제한을 넘었습니다.".into()); }
    let mut ids = std::collections::HashSet::new();
    let mut output = Vec::new();
    for seat in input {
        let object = seat.as_object().ok_or("좌석 배치 항목 형식이 올바르지 않습니다.")?;
        if object.len() > 64 { return Err("좌석 배치 항목 수 제한을 넘었습니다.".into()); }
        if seat["exited"] == true { continue; }
        let id = seat["surface_id"].as_u64().filter(|id| *id > 0 && *id <= u32::MAX as u64)
            .ok_or("좌석 식별자가 올바르지 않습니다.")?;
        if !ids.insert(id) { return Err("중복된 좌석 식별자가 있습니다.".into()); }
        let mut allowed = Map::new();
        allowed.insert("surface_id".into(), Value::from(id));
        for key in ["role","agent","cwd","live_cwd","title","label","model"] {
            if let Some(value) = object.get(key) {
                if !value.is_null() && !value.as_str().is_some_and(valid_text) { return Err("좌석 배치 문자열 형식이나 길이가 올바르지 않습니다.".into()); }
                allowed.insert(key.into(), value.clone());
            }
        }
        for key in ["agent_alive","launch_complete"] {
            if let Some(value) = object.get(key) {
                if !value.is_null() && !value.is_boolean() { return Err("좌석 배치 상태 형식이 올바르지 않습니다.".into()); }
                allowed.insert(key.into(), value.clone());
            }
        }
        let shell = object.get("cmd").and_then(|cmd| {
            cmd.as_str().or_else(|| cmd.as_array().filter(|args| args.len() == 1).and_then(|args| args[0].as_str()))
        }).filter(|shell| known_shell(shell));
        allowed.insert("cmd".into(), shell.map(|shell| Value::Array(vec![Value::String(shell.into())])).unwrap_or(Value::Null));
        output.push(Value::Object(allowed));
    }
    Ok(output)
}

fn valid_text(value: &str) -> bool { value.len() <= MAX_TEXT && !value.chars().any(|c| c.is_control()) }
fn known_shell(shell: &str) -> bool {
    ["sh","bash","zsh","fish","dash","ksh","csh","tcsh","pwsh","pwsh.exe","powershell","powershell.exe","cmd","cmd.exe",
        "/bin/sh","/bin/bash","/bin/zsh","/bin/dash","/bin/ksh","/bin/csh","/bin/tcsh",
        "/usr/bin/sh","/usr/bin/bash","/usr/bin/zsh","/usr/bin/fish","/usr/bin/dash","/usr/bin/ksh",
        "/usr/local/bin/fish","/opt/homebrew/bin/fish"].contains(&shell)
        || [r"C:\Windows\System32\cmd.exe",r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            r"C:\Program Files\PowerShell\7\pwsh.exe"].iter().any(|known| shell.eq_ignore_ascii_case(known))
}

fn reference(path: &Path, envelope: &Envelope) -> Reference {
    Reference { path:path.into(), id:envelope.id.clone(), digest:envelope.digest.clone() }
}
fn hex(value: &str, length: usize) -> bool { value.len() == length && value.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) }
fn validate_reference(dir: &Path, reference: &Reference) -> Result<(), String> {
    if !dir.is_absolute() || ![dir.join(FILE_NAME), dir.join(format!("{FILE_NAME}.new"))]
        .iter().any(|path| path.as_os_str() == reference.path.as_os_str()) || !hex(&reference.id,32) || !hex(&reference.digest,64) {
        return Err("배치 기록 참조가 고정된 전용 경로와 일치하지 않습니다.".into());
    }
    Ok(())
}
fn referenced(dir: &Path, reference: &Reference) -> Result<(Envelope, Vec<u8>), String> {
    validate_reference(dir, reference)?;
    let (envelope, bytes) = read_owned(dir, &reference.path)?.ok_or("배치 기록이 없습니다.")?;
    if envelope.id != reference.id || envelope.digest != reference.digest { return Err("배치 기록이 참조 후 수정됐습니다.".into()); }
    Ok((envelope, bytes))
}

fn canonical(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(map.into_iter().collect::<std::collections::BTreeMap<_,_>>()
            .into_iter().map(|(key,value)| (key,canonical(value))).collect()),
        Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
        other => other,
    }
}
fn encoding(envelope: &Envelope) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&canonical(serde_json::to_value(envelope).map_err(|_| "배치 기록 변환 실패")?))
        .map_err(|_| "배치 기록 인코딩 실패".into())
}
fn seal(envelope: &mut Envelope) -> Result<(), String> {
    let mut value = serde_json::to_value(&*envelope).map_err(|_| "배치 무결성 변환 실패")?;
    value.as_object_mut().ok_or("배치 기록 형식 오류")?.remove("digest");
    let text = serde_json::to_string(&canonical(value)).map_err(|_| "배치 무결성 인코딩 실패")?;
    envelope.digest = crate::pack::content_hash_pub(&text);
    Ok(())
}
fn validate(envelope: &Envelope) -> Result<(), String> {
    if envelope.schema != 1 || !hex(&envelope.id,32) || !hex(&envelope.digest,64)
        || !envelope.socket.is_absolute() || !envelope.socket.to_str().is_some_and(valid_text)
        || envelope.daemon.pid == 0 || envelope.daemon.start_time == 0 || !envelope.daemon.executable.is_absolute()
        || !envelope.daemon.executable.to_str().is_some_and(valid_text) || envelope.captured_at == 0
        || (envelope.state == State::Pending && envelope.consumed_from.is_some())
        || (envelope.state == State::Consumed && !envelope.consumed_from.as_deref().is_some_and(|d| hex(d,64))) {
        return Err("배치 기록의 고정 형식이 올바르지 않습니다.".into());
    }
    if sanitize(&Value::Array(envelope.seats.clone()))? != envelope.seats { return Err("배치 기록에 허용되지 않은 필드가 있습니다.".into()); }
    let mut expected = envelope.clone();
    seal(&mut expected)?;
    if expected.digest != envelope.digest { return Err("배치 기록이 수정됐습니다. 원본을 보존합니다.".into()); }
    Ok(())
}

// Storage. Unix only: the shipped 0.3.1 Windows updater exits before the old GUI's plain drain
// runs, so no Windows producer exists; Windows fails closed to the existing restore + notice.
#[cfg(not(unix))]
const NO_STORAGE: &str = "이 플랫폼에서는 배치 기록을 쓰지 않고 기존 복원을 사용합니다.";

#[cfg(unix)]
mod store {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};

    pub fn random_id() -> Result<String, String> {
        let mut raw = [0u8; 16];
        File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut raw))
            .map_err(|_| "배치 기록 식별자를 만들지 못했습니다.")?;
        Ok(raw.iter().map(|b| format!("{b:02x}")).collect())
    }
    // SAFETY: geteuid has no preconditions and cannot fail.
    fn euid() -> u32 { unsafe { libc::geteuid() } }

    pub fn ensure_directory(dir: &Path) -> Result<(), String> {
        if !dir.is_absolute() { return Err("배치 기록 경로가 절대 경로가 아닙니다.".into()); }
        match std::fs::symlink_metadata(dir) {
            Ok(meta) if meta.is_dir() && meta.uid() == euid() => Ok(()),
            Ok(_) => Err("배치 기록 폴더가 안전하지 않아 기존 복원을 사용합니다.".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::DirBuilder::new().mode(0o700).create(dir)
                .map_err(|_| "배치 기록 폴더를 만들지 못했습니다.".into()),
            Err(_) => Err("배치 기록 폴더를 확인하지 못했습니다.".into()),
        }
    }

    /// Bounded, no-follow, owner/mode-checked read. Missing file = Ok(None).
    pub fn raw_read(dir: &Path, path: &Path) -> Result<Option<Vec<u8>>, String> {
        if path.parent() != Some(dir) { return Err("배치 기록 경로가 전용 폴더 밖입니다.".into()); }
        match std::fs::symlink_metadata(dir) {
            Ok(m) if m.is_dir() => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            _ => return Err("배치 기록 폴더가 안전하지 않습니다.".into()),
        }
        let file = match OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("배치 기록을 안전하게 열지 못했습니다.".into()),
        };
        let meta = file.metadata().map_err(|_| "배치 기록 정보를 읽지 못했습니다.")?;
        if !meta.is_file() || meta.uid() != euid() || meta.mode() & 0o077 != 0 || meta.len() > MAX_BYTES as u64 {
            return Err("배치 기록의 형식·소유·권한이 올바르지 않습니다.".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| "배치 기록을 읽지 못했습니다.")?;
        if bytes.len() > MAX_BYTES { return Err("배치 기록이 너무 큽니다.".into()); }
        Ok(Some(bytes))
    }

    pub fn publish(dir: &Path, path: &Path, bytes: &[u8], previous: Option<&[u8]>) -> Result<(), String> {
        if path.parent() != Some(dir) { return Err("배치 기록 경로가 전용 폴더 밖입니다.".into()); }
        let temp = dir.join(format!(".{FILE_NAME}.tmp-{}", random_id()?));
        let result = (|| {
            let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600)
                .custom_flags(libc::O_NOFOLLOW).open(&temp).map_err(|_| "임시 배치 기록을 만들지 못했습니다.")?;
            file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600)).map_err(|_| "권한 설정 실패")?;
            file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|_| "배치 기록을 쓰지 못했습니다.")?;
            if raw_read(dir, path)?.as_deref() != previous {
                return Err("배치 기록이 쓰는 동안 바뀌어 덮어쓰지 않았습니다.".to_string());
            }
            std::fs::rename(&temp, path).map_err(|_| "배치 기록을 게시하지 못했습니다.")?;
            File::open(dir).and_then(|d| d.sync_all()).map_err(|_| "배치 기록 폴더 동기화 실패")?;
            Ok(())
        })();
        if result.is_err() { let _ = std::fs::remove_file(&temp); }
        result
    }

    #[cfg(test)]
    pub fn test_root(raw: Option<std::ffi::OsString>) -> Result<PathBuf, String> {
        use std::path::Component;
        let raw = PathBuf::from(raw.ok_or("WAVE_TEST_ROOT 가 지정되지 않았습니다.")?);
        if !raw.is_absolute() || raw.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir))
            || raw.components().any(|c| c.as_os_str() == ".cys") {
            return Err("WAVE_TEST_ROOT 경로가 안전하지 않습니다.".into());
        }
        let meta = std::fs::symlink_metadata(&raw).map_err(|_| "WAVE_TEST_ROOT 가 없습니다.")?;
        if !meta.is_dir() || meta.uid() != euid() || meta.mode() & 0o777 != 0o700 {
            return Err("WAVE_TEST_ROOT 는 내 소유 0700 폴더여야 합니다.".into());
        }
        let real = raw.canonicalize().map_err(|_| "WAVE_TEST_ROOT 확인 실패")?;
        for protected in ["/", "/Applications", "/usr", "/opt", "/bin", "/sbin", "/Library", "/System", "/etc"] {
            let protected = Path::new(protected);
            if real == protected || (protected != Path::new("/") && real.starts_with(protected)) {
                return Err("WAVE_TEST_ROOT 가 시스템 경로입니다.".into());
            }
        }
        Ok(real)
    }
}

#[cfg(unix)]
fn random_id() -> Result<String, String> { store::random_id() }
#[cfg(unix)]
fn ensure_directory(dir: &Path) -> Result<(), String> { store::ensure_directory(dir) }
#[cfg(unix)]
fn read_owned(dir: &Path, path: &Path) -> Result<Option<(Envelope, Vec<u8>)>, String> {
    let Some(bytes) = store::raw_read(dir, path)? else { return Ok(None) };
    let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|_| "배치 기록 형식이 올바르지 않습니다.")?;
    validate(&envelope)?;
    if encoding(&envelope)? != bytes { return Err("배치 기록이 수정됐습니다. 원본을 보존합니다.".into()); }
    Ok(Some((envelope, bytes)))
}
#[cfg(unix)]
fn publish(dir: &Path, path: &Path, envelope: &Envelope, previous: Option<&[u8]>) -> Result<(), String> {
    store::publish(dir, path, &encoding(envelope)?, previous)
}
#[cfg(unix)]
fn raw_bytes(dir: &Path, path: &Path) -> Result<Option<Vec<u8>>, String> { store::raw_read(dir, path) }
#[cfg(unix)]
fn publish_bytes(dir: &Path, path: &Path, bytes: &[u8], previous: Option<&[u8]>) -> Result<(), String> {
    store::publish(dir, path, bytes, previous)
}
#[cfg(all(test, unix))]
use store::test_root;

#[cfg(not(unix))]
fn random_id() -> Result<String, String> { Err(NO_STORAGE.into()) }
#[cfg(not(unix))]
fn ensure_directory(_dir: &Path) -> Result<(), String> { Err(NO_STORAGE.into()) }
#[cfg(not(unix))]
fn read_owned(_dir: &Path, _path: &Path) -> Result<Option<(Envelope, Vec<u8>)>, String> { Err(NO_STORAGE.into()) }
#[cfg(not(unix))]
fn raw_bytes(_dir: &Path, _path: &Path) -> Result<Option<Vec<u8>>, String> { Ok(None) } // no marker store: load keeps its existing notice
#[cfg(not(unix))]
fn publish_bytes(_dir: &Path, _path: &Path, _bytes: &[u8], _previous: Option<&[u8]>) -> Result<(), String> { Err(NO_STORAGE.into()) }
#[cfg(not(unix))]
fn publish(_dir: &Path, _path: &Path, _envelope: &Envelope, _previous: Option<&[u8]>) -> Result<(), String> { Err(NO_STORAGE.into()) }

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = test_root(std::env::var_os("WAVE_TEST_ROOT")).expect("explicit private test root required");
            // Parallel tests: the clock alone can repeat (microsecond granularity on macOS), so add a counter.
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let directory = root.join(format!("update-snapshot-{}-{}-{}", std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
            std::fs::DirBuilder::new().mode(0o700).create(&directory).unwrap();
            Self(directory)
        }
        fn daemon(&self) -> ProcessIdentity {
            ProcessIdentity { pid:123, start_time:456, executable:self.0.join("waved") }
        }
        fn surfaces(&self) -> Value {
            json!({"surfaces":[{"surface_id":7,"role":null,"agent":null,"agent_alive":null,
                "launch_complete":false,"cwd":self.0,"live_cwd":self.0,"title":"plain","label":"seat",
                "model":null,"cmd":["/bin/zsh"],"env":{"API_KEY":"MUST-NOT-PERSIST"},
                "claude_config_dir":"MUST-NOT-PERSIST","session":"MUST-NOT-PERSIST",
                "launch_token":"MUST-NOT-PERSIST","transcript":"MUST-NOT-PERSIST",
                "scrollback":"MUST-NOT-PERSIST","usage":{"tokens":"MUST-NOT-PERSIST"}}]})
        }
    }
    impl Drop for Fixture { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).unwrap(); } }

    #[test]
    fn root_refuses_unset_and_protected_locations_before_io() {
        assert!(test_root(None).is_err());
        for path in [".","/Applications","/usr/local/bin","/opt/homebrew/bin","/Users/user/.cys"] {
            assert!(test_root(Some(path.into())).is_err());
        }
    }

    #[test]
    fn write_status_consume_once_and_drop_secret_fields() {
        let fixture = Fixture::new();
        let socket = fixture.0.join("socket");
        let reference = write_at(&fixture.0, &socket, &fixture.daemon(), &fixture.surfaces()).unwrap();
        let bytes = std::fs::read_to_string(&reference.path).unwrap();
        assert!(!bytes.contains("MUST-NOT-PERSIST"));
        assert_eq!(std::fs::metadata(&reference.path).unwrap().permissions().mode() & 0o777, 0o600);
        let Load::Ready(pending) = load_at(&fixture.0, &socket) else { panic!("pending metadata missing") };
        assert_eq!(pending.drain_status, None);
        assert_eq!(pending.seats[0]["cmd"], json!(["/bin/zsh"]));
        let reference = finish_drain_at(&fixture.0, &reference, 9).unwrap();
        let Load::Ready(pending) = load_at(&fixture.0, &socket) else { panic!("pending status missing") };
        assert_eq!(pending.drain_status, Some(9));
        mark_consumed_at(&fixture.0, &reference).unwrap();
        mark_consumed_at(&fixture.0, &reference).unwrap();
        assert!(reference.path.is_file());
        assert!(matches!(load_at(&fixture.0, &socket), Load::Fallback(_)));
    }

    #[test]
    fn corrupt_snapshot_falls_back_and_new_file_preserves_edited_bytes() {
        let fixture = Fixture::new();
        let socket = fixture.0.join("socket");
        let reference = write_at(&fixture.0, &socket, &fixture.daemon(), &fixture.surfaces()).unwrap();
        std::fs::write(&reference.path, "learner-edited: keep these bytes").unwrap();
        let Load::Fallback(notice) = load_at(&fixture.0, &socket) else { panic!("corrupt must fall back") };
        assert!(notice.contains("복원"));
        let replacement = write_at(&fixture.0, &socket, &fixture.daemon(), &fixture.surfaces()).unwrap();
        assert_eq!(replacement.path, fixture.0.join(format!("{FILE_NAME}.new")));
        assert_eq!(std::fs::read_to_string(&reference.path).unwrap(), "learner-edited: keep these bytes");
        assert!(matches!(load_at(&fixture.0, &socket), Load::Ready(_)));
        assert!(mark_consumed_at(&fixture.0, &reference).is_err());
    }

    #[test]
    fn symlink_target_is_untouched_and_reference_cannot_escape_reserved_names() {
        let fixture = Fixture::new();
        let target = fixture.0.join("owner-data");
        std::fs::write(&target, "KEEP").unwrap();
        std::os::unix::fs::symlink(&target, fixture.0.join(FILE_NAME)).unwrap();
        let reference = write_at(&fixture.0, &fixture.0.join("socket"), &fixture.daemon(), &fixture.surfaces()).unwrap();
        assert!(reference.path.ends_with(format!("{FILE_NAME}.new")));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "KEEP");
        let escaped = Reference { path:target.clone(), ..reference };
        assert!(mark_consumed_at(&fixture.0, &escaped).is_err());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "KEEP");
    }

    #[test]
    fn drain_snapshot_failure_is_recorded_and_forces_the_fallback_notice() {
        let fixture = Fixture::new();
        let socket = fixture.0.join("socket");
        let marker = fixture.0.join(FAILED_NAME);
        // An earlier drain left a pending snapshot that was never applied.
        drain_begin_at(&fixture.0, &socket, || Ok((fixture.daemon(), fixture.surfaces()))).expect("baseline snapshot");
        assert!(matches!(load_at(&fixture.0, &socket), Load::Ready(_)));
        // This drain cannot capture (daemon unreachable): no reference, failure recorded, stale snapshot not used.
        assert!(drain_begin_at(&fixture.0, &socket, || Err("system.identify 실패".into())).is_none());
        let Load::Fallback(notice) = load_at(&fixture.0, &socket) else { panic!("stale snapshot used after a recorded drain failure") };
        assert_eq!(notice, DRAIN_FAILED_NOTICE);
        let bytes = std::fs::read_to_string(&marker).expect("failure marker written");
        assert_eq!(std::fs::metadata(&marker).unwrap().permissions().mode() & 0o777, 0o600);
        let value: Value = serde_json::from_str(&bytes).unwrap();
        assert_eq!(value["stage"], "begin");
        assert_eq!(value.as_object().unwrap().keys().cloned().collect::<Vec<_>>(), ["at","schema","socket","stage"], "metadata only");
        assert!(!bytes.contains("MUST-NOT-PERSIST"));
        // The marker names its own service: another socket keeps its ordinary notice.
        let other = fixture.0.join("other");
        assert!(matches!(load_at(&fixture.0, &other), Load::Fallback(n) if n != DRAIN_FAILED_NOTICE));
        // A later successful drain clears the marker; its finish keeps the snapshot usable.
        let reference = drain_begin_at(&fixture.0, &socket, || Ok((fixture.daemon(), fixture.surfaces())));
        assert!(!marker.exists());
        drain_finish_at(&fixture.0, &socket, reference, 1);
        let Load::Ready(pending) = load_at(&fixture.0, &socket) else { panic!("fresh snapshot must load") };
        assert_eq!(pending.drain_status, Some(1));
        // A write failure (both slots hold foreign bytes) is recorded the same way.
        for name in [FILE_NAME.to_string(), format!("{FILE_NAME}.new")] { std::fs::write(fixture.0.join(name), "foreign").unwrap(); }
        assert!(drain_begin_at(&fixture.0, &socket, || Ok((fixture.daemon(), fixture.surfaces()))).is_none());
        assert!(matches!(load_at(&fixture.0, &socket), Load::Fallback(n) if n == DRAIN_FAILED_NOTICE));
    }

    #[test]
    fn drain_finish_failure_is_recorded_and_a_corrupt_marker_fails_closed() {
        let fixture = Fixture::new();
        let socket = fixture.0.join("socket");
        let reference = drain_begin_at(&fixture.0, &socket, || Ok((fixture.daemon(), fixture.surfaces()))).unwrap();
        std::fs::write(&reference.path, "edited between begin and finish").unwrap();
        drain_finish_at(&fixture.0, &socket, Some(reference.clone()), 0);
        assert_eq!(std::fs::read_to_string(&reference.path).unwrap(), "edited between begin and finish");
        let value: Value = serde_json::from_slice(&std::fs::read(fixture.0.join(FAILED_NAME)).unwrap()).unwrap();
        assert_eq!(value["stage"], "finish");
        assert!(matches!(load_at(&fixture.0, &socket), Load::Fallback(n) if n == DRAIN_FAILED_NOTICE));
        // No reference (begin already failed and recorded): finish is a no-op.
        std::fs::remove_file(fixture.0.join(FAILED_NAME)).unwrap();
        drain_finish_at(&fixture.0, &socket, None, 1);
        assert!(!fixture.0.join(FAILED_NAME).exists());
        // An unreadable marker still forces the fallback.
        std::fs::write(fixture.0.join(FAILED_NAME), "junk").unwrap();
        assert!(matches!(load_at(&fixture.0, &socket), Load::Fallback(n) if n == DRAIN_FAILED_NOTICE));
    }

    #[test]
    fn custom_command_arguments_are_never_restorable() {
        let seats = sanitize(&json!({"surfaces":[{"surface_id":1,"cmd":["bash","-c","secret-command"]}]})).unwrap();
        assert!(seats[0]["cmd"].is_null());
        assert!(sanitize(&json!({"surfaces":[{"surface_id":"bad"}]})).is_err());
    }

    #[test]
    fn wrong_socket_world_readable_and_oversize_files_fall_back_untouched() {
        let fixture = Fixture::new();
        let socket = fixture.0.join("socket");
        let reference = write_at(&fixture.0, &socket, &fixture.daemon(), &fixture.surfaces()).unwrap();
        assert!(matches!(load_at(&fixture.0, &fixture.0.join("other")), Load::Fallback(_)));
        let good = std::fs::read(&reference.path).unwrap();
        std::fs::set_permissions(&reference.path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(load_at(&fixture.0, &socket), Load::Fallback(_)));
        assert_eq!(std::fs::read(&reference.path).unwrap(), good);
        std::fs::set_permissions(&reference.path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(&reference.path, vec![b'x'; MAX_BYTES + 1]).unwrap();
        std::fs::set_permissions(&reference.path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(load_at(&fixture.0, &socket), Load::Fallback(_)));
        assert_eq!(std::fs::metadata(&reference.path).unwrap().len() as usize, MAX_BYTES + 1);
    }

    #[test]
    fn whitespace_edit_of_pending_file_is_detected_and_preserved() {
        let fixture = Fixture::new();
        let socket = fixture.0.join("socket");
        let reference = write_at(&fixture.0, &socket, &fixture.daemon(), &fixture.surfaces()).unwrap();
        let edited = format!("{}\n", std::fs::read_to_string(&reference.path).unwrap());
        std::fs::write(&reference.path, &edited).unwrap();
        assert!(matches!(load_at(&fixture.0, &socket), Load::Fallback(_)));
        assert_eq!(std::fs::read_to_string(&reference.path).unwrap(), edited);
        assert!(finish_drain_at(&fixture.0, &reference, 0).is_err());
    }

    #[test]
    fn symlinked_state_directory_is_refused() {
        let fixture = Fixture::new();
        let real = fixture.0.join("real");
        std::fs::create_dir(&real).unwrap();
        let link = fixture.0.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(write_at(&link, &fixture.0.join("socket"), &fixture.daemon(), &fixture.surfaces()).is_err());
        assert_eq!(std::fs::read_dir(&real).unwrap().count(), 0);
    }

    #[test]
    fn missing_directory_is_created_private_and_missing_file_falls_back() {
        let fixture = Fixture::new();
        let dir = fixture.0.join("fresh");
        assert!(matches!(load_at(&dir, &fixture.0.join("socket")), Load::Fallback(_)));
        write_at(&dir, &fixture.0.join("socket"), &fixture.daemon(), &fixture.surfaces()).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
    }
}
