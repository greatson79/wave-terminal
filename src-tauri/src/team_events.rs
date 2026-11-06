use serde_json::{json, Value};

/// Shared by the live daemon stream and the CLI-to-sidebar notification regression.
pub fn forward(event: &Value, mut emit: impl FnMut(&str, Value)) {
    if cys::teams::is_team_change_event(event) {
        emit("teams-changed", json!({}));
    }
}
