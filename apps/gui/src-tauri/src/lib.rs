use std::sync::{Arc, Mutex};

use sukaku_forge_app::port::ApplicationPort;

#[cfg_attr(not(feature = "desktop-runtime"), allow(dead_code))]
type PortState = Arc<Mutex<ApplicationPort>>;

#[cfg_attr(not(feature = "desktop-runtime"), allow(dead_code))]
fn dispatch_on_state(state: &PortState, request: String) -> Result<String, String> {
    let mut port = state
        .lock()
        .map_err(|_| "application port state is unavailable".to_owned())?;
    Ok(port.dispatch_json(&request))
}

#[cfg(feature = "tauri-adapter")]
#[cfg_attr(not(feature = "desktop-runtime"), allow(dead_code))]
#[tauri::command]
async fn dispatch_json(
    state: tauri::State<'_, PortState>,
    request: String,
) -> Result<String, String> {
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || dispatch_on_state(&state, request))
        .await
        .map_err(|error| format!("application dispatch task failed: {error}"))?
}

#[cfg(feature = "desktop-runtime")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Arc::new(Mutex::new(ApplicationPort::new())))
        .invoke_handler(tauri::generate_handler![dispatch_json])
        .run(tauri::generate_context!())
        .expect("error while running the Sukaku Forge Tauri application");
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use sukaku_forge_app::port::PROTOCOL_VERSION;

    use super::{ApplicationPort, Arc, Mutex, dispatch_on_state};

    #[test]
    fn shared_port_state_survives_across_dispatches() {
        let state = Arc::new(Mutex::new(ApplicationPort::new()));
        let created = dispatch_on_state(
            &state,
            json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": 1,
                "command": "create_session",
                "puzzle": "12345678........................................................................."
            })
            .to_string(),
        )
        .unwrap();
        let created: Value = serde_json::from_str(&created).unwrap();
        assert_eq!(created["response"], "session_created");

        let hinted = dispatch_on_state(
            &state,
            json!({
                "protocol_version": PROTOCOL_VERSION,
                "request_id": 2,
                "command": "next_hint",
                "expected_revision": created["snapshot"]["revision"]
            })
            .to_string(),
        )
        .unwrap();
        let hinted: Value = serde_json::from_str(&hinted).unwrap();
        assert_eq!(hinted["response"], "next_hint");
        assert_eq!(hinted["outcome"], "presented");
        assert_eq!(
            hinted["effects"]["placement"],
            json!({ "cell": 8, "digit": 9 })
        );
    }
}
