use cys::teams::{
    io_error, learner_store, validate_folder, StoreStatus, Team, TeamError, TeamPatch,
};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;

type Result<T> = std::result::Result<T, TeamError>;

#[tauri::command]
pub async fn team_list() -> Result<Vec<Team>> {
    tokio::task::spawn_blocking(|| learner_store().list())
        .await
        .map_err(|_| io_error())?
}

#[tauri::command]
pub async fn team_create(app: AppHandle, name: String, folder: String) -> Result<Team> {
    let team = tokio::task::spawn_blocking(move || learner_store().create(name, folder))
        .await
        .map_err(|_| io_error())??;
    emit_changed(&app);
    Ok(team)
}

#[tauri::command]
pub async fn team_update(app: AppHandle, id: String, patch: TeamPatch) -> Result<Team> {
    let team = tokio::task::spawn_blocking(move || learner_store().update(id, patch))
        .await
        .map_err(|_| io_error())??;
    emit_changed(&app);
    Ok(team)
}

#[tauri::command]
pub async fn team_delete(app: AppHandle, id: String) -> Result<()> {
    tokio::task::spawn_blocking(move || learner_store().delete(id))
        .await
        .map_err(|_| io_error())??;
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn team_mark_starters_offered(app: AppHandle) -> Result<()> {
    tokio::task::spawn_blocking(|| learner_store().mark_starters_offered())
        .await
        .map_err(|_| io_error())??;
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn team_store_status() -> Result<StoreStatus> {
    tokio::task::spawn_blocking(|| learner_store().status())
        .await
        .map_err(|_| io_error())?
}

#[tauri::command]
pub async fn team_pick_folder(app: AppHandle, initial: Option<String>) -> Result<Option<String>> {
    tokio::task::spawn_blocking(move || {
        let mut picker = app.dialog().file().set_title("작업 폴더 선택");
        if let Some(initial) = picker_initial(initial) {
            picker = picker.set_directory(initial);
        }
        selected_folder(picker.blocking_pick_folder())
    })
    .await
    .map_err(|_| io_error())?
}

fn picker_initial(initial: Option<String>) -> Option<String> {
    initial.filter(|folder| validate_folder(folder).is_ok())
}

fn selected_folder(selected: Option<tauri_plugin_dialog::FilePath>) -> Result<Option<String>> {
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected.into_path().map_err(|_| io_error())?;
    let folder = path.to_str().ok_or_else(io_error)?;
    validate_folder(folder)?;
    Ok(Some(folder.into()))
}

pub fn emit_changed(app: &AppHandle) {
    let _ = app.emit("teams-changed", serde_json::json!({}));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_picker_cancel_and_selection_validate_without_creating_a_team() {
        assert_eq!(picker_initial(Some("relative".into())), None);
        assert_eq!(
            picker_initial(Some(
                std::env::temp_dir()
                    .join("missing-ca-picker-hint")
                    .to_string_lossy()
                    .into_owned()
            )),
            None
        );
        assert_eq!(selected_folder(None).unwrap(), None);
        let folder = std::env::temp_dir();
        let selected =
            selected_folder(Some(tauri_plugin_dialog::FilePath::Path(folder.clone()))).unwrap();
        assert_eq!(selected, Some(folder.to_str().unwrap().into()));
        assert_eq!(
            selected_folder(Some(tauri_plugin_dialog::FilePath::Path("relative".into())))
                .unwrap_err()
                .code,
            "invalid_folder"
        );
    }

    #[test]
    fn team_forwarder_maps_only_team_changed_events() {
        assert!(cys::teams::is_team_change_event(
            &serde_json::json!({"type":"event", "name":"team.changed"})
        ));
        assert!(!cys::teams::is_team_change_event(
            &serde_json::json!({"type":"event", "name":"surface.changed"})
        ));
        assert!(!cys::teams::is_team_change_event(
            &serde_json::json!({"type":"response", "name":"team.changed"})
        ));
        let wiring = include_str!("main.rs");
        assert!(wiring.contains("team_events::forward(&v, |name, payload| {"));
    }
}
