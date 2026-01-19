#[cfg(windows)]
mod j2534;

#[cfg(windows)]
mod bridge_client;

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

#[cfg(windows)]
use j2534::{
    CANMessage, ConnectionStatus, J2534Config, J2534Connection, J2534Device,
    J2534VersionInfo, PASS_FILTER, BLOCK_FILTER, FLOW_CONTROL_FILTER,
};

#[cfg(not(windows))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534Device {
    pub name: String,
    pub vendor: String,
    pub dll_path: String,
    pub can_iso15765: bool,
    pub can_iso11898: bool,
    pub compatible: bool,
}

#[cfg(not(windows))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534Config {
    pub device_name: String,
    pub baud_rate: u32,
    pub protocol: String,
    pub use_extended_id: bool,
}

#[cfg(not(windows))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CANMessage {
    pub timestamp_us: u64,
    pub arb_id: u32,
    pub extended: bool,
    pub data: Vec<u8>,
}

#[cfg(not(windows))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub connected: bool,
    pub device_name: String,
    pub baud_rate: u32,
    pub messages_sent: u64,
    pub messages_received: u64,
}

#[cfg(not(windows))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534Progress {
    pub step: String,
    pub status: String,
    pub message: Option<String>,
}

#[cfg(not(windows))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534VersionInfo {
    pub firmware_version: String,
    pub dll_version: String,
    pub api_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    pub arb_id: u32,
    pub data: Vec<u8>,
    pub extended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub platform: String,
    pub arch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodicMessageRequest {
    pub arb_id: u32,
    pub data: Vec<u8>,
    pub interval_ms: u32,
    pub extended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterRequest {
    pub filter_type: String, // "pass", "block", or "flow_control"
    pub mask: Vec<u8>,
    pub pattern: Vec<u8>,
    pub extended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigRequest {
    pub parameter: u32,
    pub value: Option<u32>,
}

pub struct AppState {
    #[cfg(windows)]
    connection: Arc<Mutex<Option<J2534Connection>>>,
    #[cfg(windows)]
    messages_sent: Arc<Mutex<u64>>,
    #[cfg(windows)]
    messages_received: Arc<Mutex<u64>>,
    #[cfg(windows)]
    device_name: Arc<Mutex<String>>,
    #[cfg(windows)]
    baud_rate: Arc<Mutex<u32>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            #[cfg(windows)]
            connection: Arc::new(Mutex::new(None)),
            #[cfg(windows)]
            messages_sent: Arc::new(Mutex::new(0)),
            #[cfg(windows)]
            messages_received: Arc::new(Mutex::new(0)),
            #[cfg(windows)]
            device_name: Arc::new(Mutex::new(String::new())),
            #[cfg(windows)]
            baud_rate: Arc::new(Mutex::new(500000)),
        }
    }
}

#[tauri::command]
fn get_platform_info() -> PlatformInfo {
    PlatformInfo {
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

#[tauri::command]
fn j2534_enumerate_devices() -> Vec<J2534Device> {
    #[cfg(windows)]
    {
        j2534::enumerate_devices()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[tauri::command]
async fn j2534_connect(
    config: J2534Config,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        // Find the device by name
        let devices = j2534::enumerate_devices();
        let device = devices
            .iter()
            .find(|d| d.name == config.device_name)
            .ok_or_else(|| format!("ERR_J2534_DEVICE_NOT_FOUND: {}", config.device_name))?;

        if !device.compatible {
            return Err("ERR_J2534_INCOMPATIBLE: Device DLL is not compatible with this process architecture".to_string());
        }

        let dll_path = device.dll_path.clone();
        let baud_rate = config.baud_rate;
        let use_extended_id = config.use_extended_id;

        // Create a channel for progress updates
        let app_handle = app.clone();

        let connection = J2534Connection::open(&dll_path, baud_rate, use_extended_id, |progress| {
            let _ = app_handle.emit("j2534-progress", progress);
        })?;

        // Store connection and config
        *state.connection.lock().unwrap() = Some(connection);
        *state.device_name.lock().unwrap() = config.device_name;
        *state.baud_rate.lock().unwrap() = baud_rate;
        *state.messages_sent.lock().unwrap() = 0;
        *state.messages_received.lock().unwrap() = 0;

        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = (config, app, state);
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_disconnect(state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut connection = state.connection.lock().unwrap();
        if connection.is_none() {
            return Err("ERR_NOT_CONNECTED: No active connection".to_string());
        }
        *connection = None;
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = state;
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
fn j2534_get_status(state: State<'_, AppState>) -> ConnectionStatus {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        ConnectionStatus {
            connected: connection.is_some(),
            device_name: state.device_name.lock().unwrap().clone(),
            baud_rate: *state.baud_rate.lock().unwrap(),
            messages_sent: *state.messages_sent.lock().unwrap(),
            messages_received: *state.messages_received.lock().unwrap(),
        }
    }

    #[cfg(not(windows))]
    {
        let _ = state;
        ConnectionStatus {
            connected: false,
            device_name: String::new(),
            baud_rate: 0,
            messages_sent: 0,
            messages_received: 0,
        }
    }
}

#[tauri::command]
async fn j2534_send_message(
    request: SendMessageRequest,
    state: State<'_, AppState>,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.send_message(request.arb_id, &request.data, request.extended)?;

        drop(connection);
        *state.messages_sent.lock().unwrap() += 1;

        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = (request, state);
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_read_messages(
    timeout_ms: u32,
    state: State<'_, AppState>,
) -> Result<Vec<CANMessage>, String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        let messages = conn.read_messages(timeout_ms)?;
        let count = messages.len() as u64;

        drop(connection);
        *state.messages_received.lock().unwrap() += count;

        Ok(messages)
    }

    #[cfg(not(windows))]
    {
        let _ = (timeout_ms, state);
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_clear_buffers(state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.clear_buffers()
    }

    #[cfg(not(windows))]
    {
        let _ = state;
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_read_version(state: State<'_, AppState>) -> Result<J2534VersionInfo, String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.read_version()
    }

    #[cfg(not(windows))]
    {
        let _ = state;
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_get_last_error(state: State<'_, AppState>) -> Result<String, String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.get_last_error()
    }

    #[cfg(not(windows))]
    {
        let _ = state;
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_read_battery_voltage(state: State<'_, AppState>) -> Result<f64, String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.read_battery_voltage()
    }

    #[cfg(not(windows))]
    {
        let _ = state;
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_read_programming_voltage(state: State<'_, AppState>) -> Result<f64, String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.read_programming_voltage()
    }

    #[cfg(not(windows))]
    {
        let _ = state;
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_start_periodic_message(
    request: PeriodicMessageRequest,
    state: State<'_, AppState>,
) -> Result<u32, String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.start_periodic_message(request.arb_id, &request.data, request.interval_ms, request.extended)
    }

    #[cfg(not(windows))]
    {
        let _ = (request, state);
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_stop_periodic_message(msg_id: u32, state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.stop_periodic_message(msg_id)
    }

    #[cfg(not(windows))]
    {
        let _ = (msg_id, state);
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_clear_periodic_messages(state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.clear_periodic_messages()
    }

    #[cfg(not(windows))]
    {
        let _ = state;
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_add_filter(request: FilterRequest, state: State<'_, AppState>) -> Result<u32, String> {
    #[cfg(windows)]
    {
        let filter_type = match request.filter_type.as_str() {
            "pass" => PASS_FILTER,
            "block" => BLOCK_FILTER,
            "flow_control" => FLOW_CONTROL_FILTER,
            _ => return Err("ERR_INVALID_FILTER_TYPE: Must be 'pass', 'block', or 'flow_control'".to_string()),
        };

        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.add_filter(filter_type, &request.mask, &request.pattern, request.extended)
    }

    #[cfg(not(windows))]
    {
        let _ = (request, state);
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_remove_filter(filter_id: u32, state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.remove_filter(filter_id)
    }

    #[cfg(not(windows))]
    {
        let _ = (filter_id, state);
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_clear_filters(state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.clear_filters()
    }

    #[cfg(not(windows))]
    {
        let _ = state;
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_get_config(parameter: u32, state: State<'_, AppState>) -> Result<u32, String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.get_config(parameter)
    }

    #[cfg(not(windows))]
    {
        let _ = (parameter, state);
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_set_config(parameter: u32, value: u32, state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.set_config(parameter, value)
    }

    #[cfg(not(windows))]
    {
        let _ = (parameter, value, state);
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_get_loopback(state: State<'_, AppState>) -> Result<bool, String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.get_loopback()
    }

    #[cfg(not(windows))]
    {
        let _ = state;
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_set_loopback(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.set_loopback(enabled)
    }

    #[cfg(not(windows))]
    {
        let _ = (enabled, state);
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_get_data_rate(state: State<'_, AppState>) -> Result<u32, String> {
    #[cfg(windows)]
    {
        let connection = state.connection.lock().unwrap();
        let conn = connection
            .as_ref()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.get_data_rate()
    }

    #[cfg(not(windows))]
    {
        let _ = state;
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_platform_info,
            j2534_enumerate_devices,
            j2534_connect,
            j2534_disconnect,
            j2534_get_status,
            j2534_send_message,
            j2534_read_messages,
            j2534_clear_buffers,
            j2534_read_version,
            j2534_get_last_error,
            j2534_read_battery_voltage,
            j2534_read_programming_voltage,
            j2534_start_periodic_message,
            j2534_stop_periodic_message,
            j2534_clear_periodic_messages,
            j2534_add_filter,
            j2534_remove_filter,
            j2534_clear_filters,
            j2534_get_config,
            j2534_set_config,
            j2534_get_loopback,
            j2534_set_loopback,
            j2534_get_data_rate,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
