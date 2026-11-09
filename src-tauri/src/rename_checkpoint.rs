use serde_json::Value;
use std::path::Path;

pub(super) fn validate(report: &Value, socket: &Path, surfaces: &Value) -> Result<(), String> {
    let invalid = || "좌석별 저장 검증 응답이 올바르지 않습니다. 기존 서비스를 유지합니다.".to_string();
    let socket = socket.to_str().filter(|s| !s.trim().is_empty()).ok_or_else(invalid)?;
    let nodes = report["nodes"].as_array().ok_or_else(invalid)?;
    if report["mode"].as_str() != Some("drain-verify")
        || report["all_saved"].as_bool() != Some(true)
        || report["total"].as_u64() != Some(nodes.len() as u64)
        || report["pending_loss_warning"].as_array().is_none_or(|a| !a.is_empty())
    {
        return Err(invalid());
    }
    let mut saved = Vec::new();
    for node in nodes {
        let socket = node["socket"].as_str().filter(|s| !s.trim().is_empty()).ok_or_else(invalid)?;
        let surface = node["surface"].as_str().ok_or_else(invalid)?;
        let sid = surface.strip_prefix("surface:").and_then(|s| s.parse::<u64>().ok()).filter(|sid| *sid > 0).ok_or_else(invalid)?;
        let role = node["role"].as_str().filter(|r| !r.trim().is_empty()).ok_or_else(invalid)?;
        if surface != format!("surface:{sid}")
            || node["outcome"].as_str() != Some("saved")
            || node["pending_undelivered"].as_u64() != Some(0)
            || saved.iter().any(|(sk, sf, _)| *sk == socket && *sf == surface)
        {
            return Err(invalid());
        }
        saved.push((socket, surface, role));
    }
    let mut expected = Vec::new();
    for surface in surfaces.as_array().ok_or_else(invalid)? {
        if surface["exited"].as_bool() != Some(false) || surface["agent_alive"].as_bool() != Some(true) {
            continue;
        }
        let role = surface["role"].as_str().filter(|r| !r.trim().is_empty());
        let agent = surface["agent"].as_str().filter(|a| !a.trim().is_empty());
        let (Some(role), Some(_)) = (role, agent) else { continue; };
        let sid = surface["surface_id"].as_u64().filter(|sid| *sid > 0).ok_or_else(invalid)?;
        if expected.contains(&sid) { return Err(invalid()); }
        expected.push(sid);
        let reference = format!("surface:{sid}");
        if !saved.iter().any(|(sk, sf, r)| *sk == socket && *sf == reference && *r == role) {
            return Err("실행 중인 좌석의 저장 기록이 빠졌거나 소켓·역할이 다릅니다. 기존 서비스를 유지합니다.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn surfaces() -> Value {
        json!([
            {"surface_id":1,"exited":false,"agent_alive":true,"role":"master","agent":"claude"},
            {"surface_id":2,"exited":false,"agent_alive":true,"role":"worker","agent":"codex"}
        ])
    }
    fn node(socket: &str, sid: u64, role: &str) -> Value {
        json!({"socket":socket,"surface":format!("surface:{sid}"),"role":role,"outcome":"saved","pending_undelivered":0})
    }
    fn report(nodes: Vec<Value>) -> Value {
        json!({"mode":"drain-verify","all_saved":true,"total":nodes.len(),"nodes":nodes,"pending_loss_warning":[]})
    }
    fn valid() -> Value {
        report(vec![node("primary",1,"master"),node("primary",2,"worker"),node("department",1,"worker")])
    }
    fn check(report: &Value, surfaces: &Value) -> Result<(), String> {
        validate(report, Path::new("primary"), surfaces)
    }

    #[test]
    fn department_successes_cannot_mask_missing_primary_checkpoint() {
        let report = report(vec![node("primary",1,"master"),node("department",2,"worker"),node("department",3,"worker")]);
        assert!(report["total"].as_u64().unwrap() >= surfaces().as_array().unwrap().len() as u64);
        assert!(check(&report,&surfaces()).is_err());
    }
    #[test]
    fn duplicate_socket_surface_pairs_are_rejected() {
        let report = report(vec![node("primary",1,"master"),node("primary",2,"worker"),node("primary",2,"worker")]);
        assert!(check(&report,&surfaces()).is_err());
    }
    #[test]
    fn wrong_socket_and_role_do_not_prove_expected_seat() {
        let mut report = valid();
        report["nodes"][1]["socket"] = json!("wrong");
        assert!(check(&report,&surfaces()).is_err());
        let mut report = valid();
        report["nodes"][1]["role"] = json!("different");
        assert!(check(&report,&surfaces()).is_err());
    }
    #[test]
    fn pending_warning_or_node_pending_messages_reject_checkpoint() {
        let mut report = valid();
        report["pending_loss_warning"] = json!([{"pending_undelivered":1}]);
        assert!(check(&report,&surfaces()).is_err());
        let mut report = valid();
        report["nodes"][1]["pending_undelivered"] = json!(1);
        assert!(check(&report,&surfaces()).is_err());
    }
    #[test]
    fn valid_multi_socket_report_preserves_department_successes() {
        assert!(check(&valid(),&surfaces()).is_ok());
    }
    #[test]
    fn malformed_or_unsuccessful_reports_fail_closed_with_korean_errors() {
        let good = valid();
        for (field,bad) in [
            ("mode",json!("drain")),("all_saved",json!(false)),("total",json!(2)),
            ("nodes",json!({})),("pending_loss_warning",json!(null)),
        ] {
            let mut report = good.clone();
            report[field] = bad;
            let error = check(&report,&surfaces()).unwrap_err();
            assert!(!error.is_ascii(),"{field}: {error}");
        }
        for field in ["mode","all_saved","total","nodes","pending_loss_warning"] {
            let mut report = good.clone();
            report.as_object_mut().unwrap().remove(field);
            assert!(check(&report,&surfaces()).is_err(),"missing {field}");
        }
        for (field,bad) in [
            ("outcome",json!("timeout")),("outcome",json!("unknown")),("outcome",json!(null)),
            ("socket",json!("")),("surface",json!("surface:02")),("surface",json!("surface:0")),
            ("role",json!("")),("pending_undelivered",json!(null)),
        ] {
            let mut report = good.clone();
            report["nodes"][2][field] = bad;
            assert!(check(&report,&surfaces()).is_err(),"{field}");
        }
        assert!(check(&json!(null),&surfaces()).is_err());
    }
    #[test]
    fn intentionally_dead_ai_and_plain_shells_are_not_checkpoint_requirements() {
        let captured = json!([
            {"surface_id":1,"exited":false,"agent_alive":true,"role":"master","agent":"claude"},
            {"surface_id":2,"exited":false,"agent_alive":false,"role":"worker","agent":"codex"},
            {"surface_id":3,"exited":true,"agent_alive":false,"role":"worker","agent":"codex"},
            {"surface_id":4,"exited":false,"agent_alive":false,"role":null,"agent":null}
        ]);
        assert!(check(&report(vec![node("primary",1,"master")]),&captured).is_ok());
        assert!(check(&report(vec![]),&json!([])).is_ok());
    }
}
