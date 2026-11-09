use cys::rename_handover::ProcessIdentity;
use serde_json::Value;

pub(super) fn validate(marker: &Value, observed: &ProcessIdentity, live: &Value) -> Result<(), String> {
    let invalid = || "업데이트 좌석 배치와 현재 서비스의 확인 정보가 다릅니다. 복원 기록을 유지합니다.".to_string();
    let record = &marker["rename_handover"];
    if record["phase"].as_str() != Some("awaiting-layout")
        || record["revision"].as_str().is_none_or(|r| r.trim().is_empty())
    {
        return Err(invalid());
    }
    let owner: ProcessIdentity = serde_json::from_value(record["new_owner"]["identity"].clone()).map_err(|_| invalid())?;
    if owner.pid <= 1 || owner.start_time == 0 || !owner.executable.is_absolute() || owner != *observed {
        return Err(invalid());
    }
    let mut old_ids = Vec::new();
    for surface in record["snapshot"]["surfaces"].as_array().ok_or_else(invalid)? {
        let sid = surface["surface_id"].as_u64().filter(|sid| *sid > 0).ok_or_else(invalid)?;
        if old_ids.contains(&sid) { return Err(invalid()); }
        old_ids.push(sid);
    }
    let mut seen_live = Vec::new();
    let mut active = Vec::new();
    for surface in live["surfaces"].as_array().ok_or_else(invalid)? {
        let sid = surface["surface_id"].as_u64().filter(|sid| *sid > 0).ok_or_else(invalid)?;
        let exited = surface["exited"].as_bool().ok_or_else(invalid)?;
        if seen_live.contains(&sid) { return Err(invalid()); }
        seen_live.push(sid);
        if !exited { active.push(sid); }
    }
    let map = record["surface_map"].as_object().ok_or_else(invalid)?;
    if map.len() != old_ids.len() { return Err(invalid()); }
    let mut destinations = Vec::new();
    for (old, new) in map {
        let sid = old.parse::<u64>().ok().filter(|sid| *sid > 0).ok_or_else(invalid)?;
        let destination = new.as_u64().filter(|sid| *sid > 0).ok_or_else(invalid)?;
        if old != &sid.to_string() || !old_ids.contains(&sid)
            || destinations.contains(&destination) || !active.contains(&destination)
        {
            return Err(invalid());
        }
        destinations.push(destination);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn observed() -> ProcessIdentity {
        ProcessIdentity { pid: 42, start_time: 100, executable: PathBuf::from(if cfg!(windows) { r"C:\fixture\waved.exe" } else { "/fixture/waved" }) }
    }
    fn marker() -> Value {
        json!({"rename_handover":{"phase":"awaiting-layout","revision":"r1","new_owner":{"identity":observed()},
            "snapshot":{"surfaces":[{"surface_id":1},{"surface_id":2}]},"surface_map":{"1":11,"2":12}}})
    }
    fn live() -> Value {
        json!({"surfaces":[{"surface_id":11,"exited":false},{"surface_id":12,"exited":false}]})
    }
    #[test]
    fn valid_two_seats_allow_legitimate_title_and_label_edits() {
        let mut live = live();
        live["surfaces"][0]["title"] = json!("renamed by learner");
        live["surfaces"][0]["label"] = json!("new label");
        assert!(validate(&marker(),&observed(),&live).is_ok());
    }
    #[test]
    fn empty_fresh_map_is_valid_for_the_exact_owner() {
        let mut marker = marker();
        marker["rename_handover"]["snapshot"]["surfaces"] = json!([]);
        marker["rename_handover"]["surface_map"] = json!({});
        assert!(validate(&marker,&observed(),&json!({"surfaces":[]})).is_ok());
    }
    #[test]
    fn changed_owner_epoch_pid_or_executable_rejects_old_map() {
        let original = observed();
        for changed in [
            ProcessIdentity {pid:43,..original.clone()},
            ProcessIdentity {start_time:101,..original.clone()},
            ProcessIdentity {executable:PathBuf::from("/fixture/other"),..original.clone()},
        ] {
            assert!(validate(&marker(),&changed,&live()).is_err());
        }
    }
    #[test]
    fn closed_or_missing_destination_rejects_acknowledgement() {
        let mut closed = live();
        closed["surfaces"][1]["exited"] = json!(true);
        assert!(validate(&marker(),&observed(),&closed).is_err());
        assert!(validate(&marker(),&observed(),&json!({"surfaces":[{"surface_id":11,"exited":false}]})).is_err());
    }
    #[test]
    fn map_must_cover_exact_old_ids_without_destination_collisions() {
        for map in [
            json!({"1":11}),json!({"1":11,"2":12,"3":13}),json!({"1":11,"2":11}),
            json!({"01":11,"2":12}),json!({"1":11,"2":99}),json!({"1":11,"2":0}),
            json!({"1":11,"2":"12"}),json!(null),
        ] {
            let mut marker = marker();
            marker["rename_handover"]["surface_map"] = map;
            assert!(validate(&marker,&observed(),&live()).is_err());
        }
    }
    #[test]
    fn duplicate_or_malformed_snapshot_and_live_ids_are_rejected() {
        let mut duplicate_snapshot = marker();
        duplicate_snapshot["rename_handover"]["snapshot"]["surfaces"][1]["surface_id"] = json!(1);
        assert!(validate(&duplicate_snapshot,&observed(),&live()).is_err());
        for bad in [json!(null),json!(0),json!("2")] {
            let mut marker = marker();
            marker["rename_handover"]["snapshot"]["surfaces"][1]["surface_id"] = bad;
            assert!(validate(&marker,&observed(),&live()).is_err());
        }
        let mut duplicate = live();
        duplicate["surfaces"].as_array_mut().unwrap().push(json!({"surface_id":11,"exited":false}));
        assert!(validate(&marker(),&observed(),&duplicate).is_err());
        assert!(validate(&marker(),&observed(),&json!({})).is_err());
    }
    #[test]
    fn malformed_phase_revision_or_owner_fails_closed_with_korean_errors() {
        for (field,bad) in [
            ("phase",json!("restoring")),("revision",json!("")),("revision",json!(null)),("new_owner",json!({})),
        ] {
            let mut marker = marker();
            marker["rename_handover"][field] = bad;
            let error=validate(&marker,&observed(),&live()).unwrap_err();
            assert!(!error.is_ascii(),"{field}: {error}");
        }
        for identity in [
            json!({"pid":1,"start_time":100,"executable":"/fixture/waved"}),
            json!({"pid":42,"start_time":0,"executable":"/fixture/waved"}),
            json!({"pid":42,"start_time":100,"executable":"relative"}),
        ] {
            let mut marker = marker();
            marker["rename_handover"]["new_owner"]["identity"] = identity;
            assert!(validate(&marker,&observed(),&live()).is_err());
        }
        assert!(validate(&json!(null),&observed(),&live()).is_err());
    }
}
