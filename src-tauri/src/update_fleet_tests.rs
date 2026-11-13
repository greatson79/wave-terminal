//! Isolated socket fixtures, never a running application or daemon.
use super::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

struct Fixture { registry: std::path::PathBuf, socket: std::path::PathBuf }
impl Fixture {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::path::PathBuf::from(std::env::var_os("WAVE_TEST_ROOT").expect("private test root required"));
        assert!(root.starts_with("/private/tmp/") && root.is_dir());
        assert_eq!(root.canonicalize().unwrap(), root, "no symlink fixture root");
        let dir = root.join(format!("fleet-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&dir).unwrap();
        Self { registry: dir.join("depts.json"), socket: dir.join("hq.sock") }
    }
    async fn read(&self) -> Result<Value, String> {
        super::read_update_fleet(&self.registry, self.socket.clone()).await
    }
}

async fn endpoint(path: &std::path::Path, status: Value) {
    assert!(path.starts_with("/private/tmp/"), "isolated socket required");
    let listener = UnixListener::bind(path).unwrap();
    tauri::async_runtime::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = String::new();
        BufReader::new(&mut stream).read_line(&mut request).await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(&request).unwrap()["method"], "org.status");
        stream.write_all(format!("{}\n", json!({"ok":true,"result":status})).as_bytes()).await.unwrap();
    });
}

fn check(reg: Value, status: Value, known: bool) {
    let fixture = Fixture::new();
    tauri::async_runtime::block_on(async {
        std::fs::write(&fixture.registry, reg.to_string()).unwrap();
        endpoint(&fixture.socket, status).await;
        let result = fixture.read().await;
        assert_eq!(result.is_ok(), known, "unknown must not become empty: {result:?}");
    });
}

#[test]
fn missing_registry_is_unknown() {
    let fixture = Fixture::new();
    tauri::async_runtime::block_on(async {
        endpoint(&fixture.socket, json!({"surfaces":[]})).await;
        assert!(fixture.read().await.is_err(), "absent inventory is not a zero-seat proof");
    });
}

#[test]
fn invalid_registry_shape_is_unknown() {
    for reg in [json!({}), json!({"depts":null}), json!({"depts":[]})] {
        check(reg, json!({"surfaces":[]}), false);
    }
}

#[test]
fn missing_or_invalid_surfaces_are_unknown() {
    for status in [json!({}), json!({"surfaces":null}), json!({"surfaces":{}})] {
        check(json!({"depts":{}}), status, false);
    }
}

#[test]
fn missing_or_invalid_seat_identity_and_exited_are_unknown() {
    for seat in [json!({"surface_id":1}), json!({"exited":false}),
        json!({"surface_id":0,"exited":false}), json!({"surface_id":1,"exited":"false"})] {
        check(json!({"depts":{}}), json!({"surfaces":[seat]}), false);
    }
}

#[test]
fn registered_offline_department_is_unknown_even_when_hq_is_live() {
    let sock = Fixture::new().registry.with_extension("offline.sock");
    check(json!({"depts":{"offline":{"socket":sock}}}),
        json!({"surfaces":[{"surface_id":1,"exited":false}]}), false);
}

#[test]
fn missing_endpoint_identity_is_unknown() {
    check(json!({"depts":{"dept":{}}}), json!({"surfaces":[]}), false);
}

#[test]
fn valid_empty_inventory_and_exited_seats_are_known() {
    check(json!({"depts":{}}), json!({"surfaces":[]}), true);
    check(json!({"depts":{}}), json!({"surfaces":[{"surface_id":1,"exited":true}]}), true);
}

#[test]
fn fleet_includes_hq_shell_and_every_registered_department() {
    let fixture = Fixture::new();
    tauri::async_runtime::block_on(async {
        let dept = fixture.registry.with_extension("dept.sock");
        std::fs::write(&fixture.registry, json!({"depts":{"dept":{"socket":dept}}}).to_string()).unwrap();
        endpoint(&fixture.socket, json!({"surfaces":[{"surface_id":1,"exited":false,"agent":null}]})).await;
        endpoint(&dept, json!({"surfaces":[{"surface_id":1,"exited":false,"agent":"codex"}]})).await;
        let result = fixture.read().await.unwrap();
        let departments = result["departments"].as_array().unwrap();
        assert_eq!(departments.len(), 2);
        let live = departments.iter().flat_map(|d| d["surfaces"].as_array().unwrap())
            .filter(|seat| seat["exited"] == false).count();
        assert_eq!(live, 2);
    });
}

#[test]
fn malformed_json_and_unreadable_registry_are_unknown() {
    let fixture = Fixture::new();
    std::fs::write(&fixture.registry, b"{broken").unwrap();
    tauri::async_runtime::block_on(async { assert!(fixture.read().await.is_err()); });
    std::fs::rename(&fixture.registry, fixture.registry.with_extension("malformed")).unwrap();
    std::fs::create_dir(&fixture.registry).unwrap();
    tauri::async_runtime::block_on(async { assert!(fixture.read().await.is_err()); });
}

#[test]
fn no_response_is_unknown_and_bounded() {
    let fixture = Fixture::new();
    tauri::async_runtime::block_on(async {
        std::fs::write(&fixture.registry, json!({"depts":{}}).to_string()).unwrap();
        let socket = fixture.socket.clone();
        let listener = UnixListener::bind(&socket).unwrap();
        tauri::async_runtime::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        });
        let result = tokio::time::timeout(std::time::Duration::from_millis(2500), fixture.read()).await;
        assert!(result.unwrap().is_err(), "unresponsive endpoint must be unknown within the reader deadline");
    });
}
