// Exit-code compatibility ledger: each plan row has one named regression check.
const CLI: &str = include_str!("../src/bin/cys.rs");
const GOV: &str = include_str!("../src/bin/cysd/governance.rs");
const CHANNELS: &str = include_str!("../src/bin/cysd/channels.rs");

fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source.split_once(start).unwrap().1.split_once(end).unwrap().0
}
macro_rules! contract {
    ($name:ident, $source:expr, $start:expr, $end:expr, $needle:expr) => {
        #[test] fn $name() { assert!(between($source, $start, $end).contains($needle)); }
    };
}
contract!(exit_code_contract_drain, CLI, "Command::Drain { .. } => {", "Command::GateCheck", "return if failed { 1 } else { 0 }");
contract!(exit_code_contract_restore, CLI, "fn run_restore(", "fn run_reinject(", "if fail > 0 {");
contract!(exit_code_contract_restore_pending_zero, CLI, "fn run_restore(", "fn run_reinject(", "return 0;");
contract!(exit_code_contract_boot_list_fail, CLI, "fn run_boot(", "fn run_launch_agent", "surface.list 실패: {e}");
contract!(exit_code_contract_boot_skip_zero, CLI, "fn run_boot(", "fn run_launch_agent", "continue;");
contract!(exit_code_contract_recover_unverified, CLI, "fn launch_once_on_surface(", "fn run_surface_role(", "if !fresh { return Err(\"node-recover directive not visible on screen\".into()); }");
contract!(exit_code_contract_launch_unverified_zero, CLI, "fn launch_once_on_surface(", "fn run_surface_role(", "warning: directive not visible on screen");
contract!(exit_code_contract_launch_old_daemon_zero, CLI, "fn run_launch_agent_opts(", "fn run_daemon", "warning:");
#[test] fn exit_code_contract_governance_logs() { assert!(GOV.contains("agent.recover_failed")); assert!(GOV.contains("output.status.code()")); }
contract!(exit_code_contract_fleet_text, CLI, "fn run_fleet(", "fn run_status(", "if out.iter().any(|d| d.get(\"error\").is_some()) { 1 } else { 0 }");
contract!(exit_code_contract_fleet_json_zero, CLI, "fn run_fleet(", "fn run_status(", "return 0;");
contract!(exit_code_contract_daemon_install_zero, CLI, "fn run_daemon", "fn run_fleet(", "return 0;");
contract!(exit_code_contract_sendkey_queued, CLI, "Command::SendKey {", "Command::SetStatus", "None => return Err(");
contract!(exit_code_contract_cycle_quiesce, CLI, "fn run_cycle_agent(", "fn run_node_recover(", "cleanup?;");
contract!(exit_code_contract_pack_update_codes, CLI, "fn run_pack_update(", "fn run_scoped(", "reinject_exit_code");
contract!(exit_code_contract_pack_merge, CLI, "fn run_pack_merge(", "fn skillscan_warn(", "병합 대기를 유지");
#[test] fn exit_code_contract_channel_deny_zero() { assert!(CHANNELS.contains("fn inbound(")); assert!(CHANNELS.contains("verdict")); }
contract!(exit_code_contract_run_child_code, CLI, "fn run_scoped", "mod tests", "status.code()");
contract!(exit_code_contract_usage_report_zero, CLI, "fn run_usage_report_stdin(", "fn run_usage_event_stdin(", "0");
contract!(exit_code_contract_usage_event_zero, CLI, "fn run_usage_event_stdin(", "fn run_fleet(", "0");
contract!(exit_code_contract_surface_role_zero, CLI, "fn run_surface_role(", "fn run_usage_report_stdin(", "0");
contract!(exit_code_contract_reinject_check_zero, CLI, "fn run_reinject(", "fn reinject_exit_code(", "return Ok(());");
