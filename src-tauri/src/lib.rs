#[cfg(windows)]
mod j2534;

#[cfg(windows)]
mod bridge_client;

#[cfg(windows)]
mod j2534_unified;

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

#[cfg(windows)]
use j2534::{
    CANMessage, ConnectionStatus, J2534Config,
    J2534VersionInfo, PASS_FILTER, BLOCK_FILTER, FLOW_CONTROL_FILTER,
};

#[cfg(windows)]
use j2534_unified::{UnifiedConnection, J2534DeviceExt};

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
pub struct J2534DeviceExt {
    pub name: String,
    pub vendor: String,
    pub dll_path: String,
    pub can_iso15765: bool,
    pub can_iso11898: bool,
    pub compatible: bool,
    pub bitness: u8,
    pub native: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SanityOptions {
    pub arb_id: u32,
    pub data: Vec<u8>,
    pub extended: bool,
    pub response_timeout_ms: u32,
    pub periodic_interval_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SanityStepResult {
    pub name: String,
    pub status: String, // "pass", "fail", "warn", "skip"
    pub message: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SanityReport {
    pub started_at: u64,
    pub completed_at: u64,
    pub duration_ms: u64,
    pub device_name: String,
    pub baud_rate: u32,
    pub use_extended_id: bool,
    pub steps: Vec<SanityStepResult>,
}

pub struct AppState {
    #[cfg(windows)]
    connection: Arc<Mutex<Option<UnifiedConnection>>>,
    #[cfg(windows)]
    devices: Arc<Mutex<Vec<J2534DeviceExt>>>,
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
            devices: Arc::new(Mutex::new(Vec::new())),
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

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn push_sanity_step(
    steps: &mut Vec<SanityStepResult>,
    app: &AppHandle,
    name: &str,
    status: &str,
    message: String,
    start: Instant,
) {
    let step = SanityStepResult {
        name: name.to_string(),
        status: status.to_string(),
        message,
        duration_ms: start.elapsed().as_millis() as u64,
    };
    let _ = app.emit("sanity-step", &step);
    steps.push(step);
}

#[tauri::command]
fn get_platform_info() -> PlatformInfo {
    PlatformInfo {
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

#[tauri::command]
fn j2534_enumerate_devices(state: State<'_, AppState>) -> Vec<J2534DeviceExt> {
    #[cfg(windows)]
    {
        let devices = UnifiedConnection::enumerate_devices();
        *state.devices.lock().unwrap() = devices.clone();
        devices
    }
    #[cfg(not(windows))]
    {
        let _ = state;
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
        let devices = state.devices.lock().unwrap();
        let device = devices
            .iter()
            .find(|d| d.name == config.device_name)
            .ok_or_else(|| format!("ERR_J2534_DEVICE_NOT_FOUND: {}", config.device_name))?
            .clone();
        drop(devices);

        let dll_path = device.dll_path.clone();
        let baud_rate = config.baud_rate;
        let use_extended_id = config.use_extended_id;

        // Note: Only CAN protocol (ID 5) is supported. Other J2534 protocols like ISO15765
        // are optional in the spec, so adapter support is inconsistent and unreliable.
        let protocol_id = 5; // PROTOCOL_CAN

        // Create a channel for progress updates
        let app_handle = app.clone();

        let connection = UnifiedConnection::open(&dll_path, protocol_id, baud_rate, use_extended_id, move |progress| {
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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

        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
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
        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        conn.get_data_rate()
    }

    #[cfg(not(windows))]
    {
        let _ = state;
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
async fn j2534_run_sanity_suite(
    options: SanityOptions,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SanityReport, String> {
    #[cfg(windows)]
    {
        if options.data.is_empty() || options.data.len() > 8 {
            return Err("ERR_INVALID_DATA_LENGTH: Sanity test data must be 1-8 bytes".to_string());
        }

        let started_at = now_millis();
        let device_name = state.device_name.lock().unwrap().clone();
        let baud_rate = *state.baud_rate.lock().unwrap();
        let mut steps: Vec<SanityStepResult> = Vec::new();

        let mut connection = state.connection.lock().unwrap();
        let conn = connection
            .as_mut()
            .ok_or("ERR_NOT_CONNECTED: No active connection")?;

        // Read version
        let start = Instant::now();
        match conn.read_version() {
            Ok(version) => {
                push_sanity_step(
                    &mut steps,
                    &app,
                    "Read Version",
                    "pass",
                    format!(
                        "FW: {}, DLL: {}, API: {}",
                        version.firmware_version, version.dll_version, version.api_version
                    ),
                    start,
                );
            }
            Err(err) => {
                push_sanity_step(&mut steps, &app, "Read Version", "fail", err, start);
            }
        }

        // Read battery voltage
        let start = Instant::now();
        match conn.read_battery_voltage() {
            Ok(voltage) => {
                push_sanity_step(
                    &mut steps,
                    &app,
                    "Read Battery Voltage",
                    "pass",
                    format!("{:.2} V", voltage),
                    start,
                );
            }
            Err(err) => {
                push_sanity_step(&mut steps, &app, "Read Battery Voltage", "fail", err, start);
            }
        }

        // Read programming voltage
        let start = Instant::now();
        match conn.read_programming_voltage() {
            Ok(voltage) => {
                push_sanity_step(
                    &mut steps,
                    &app,
                    "Read Programming Voltage",
                    "pass",
                    format!("{:.2} V", voltage),
                    start,
                );
            }
            Err(err) => {
                push_sanity_step(
                    &mut steps,
                    &app,
                    "Read Programming Voltage",
                    "fail",
                    err,
                    start,
                );
            }
        }

        // Clear buffers
        let start = Instant::now();
        match conn.clear_buffers() {
            Ok(()) => {
                push_sanity_step(
                    &mut steps,
                    &app,
                    "Clear Buffers",
                    "pass",
                    "Buffers cleared".to_string(),
                    start,
                );
            }
            Err(err) => {
                push_sanity_step(&mut steps, &app, "Clear Buffers", "fail", err, start);
            }
        }

        // Get data rate
        let start = Instant::now();
        match conn.get_data_rate() {
            Ok(rate) => {
                push_sanity_step(
                    &mut steps,
                    &app,
                    "Get Data Rate",
                    "pass",
                    format!("{} bps", rate),
                    start,
                );
            }
            Err(err) => {
                push_sanity_step(&mut steps, &app, "Get Data Rate", "fail", err, start);
            }
        }

        // Loopback support (warn if unsupported)
        let start = Instant::now();
        let mut loopback_supported = false;
        let mut original_loopback: Option<bool> = None;
        let mut loopback_enabled = false;
        let mut loopback_status = "warn";
        let mut loopback_message;

        match conn.get_loopback() {
            Ok(state) => {
                loopback_supported = true;
                original_loopback = Some(state);
                loopback_message = format!("Loopback reported {}", if state { "ON" } else { "OFF" });
            }
            Err(err) => {
                loopback_message = format!("Loopback not supported: {}", err);
            }
        }

        if loopback_supported {
            match conn.set_loopback(true) {
                Ok(()) => {
                    match conn.get_loopback() {
                        Ok(state) => {
                            if state {
                                loopback_enabled = true;
                                loopback_status = "pass";
                                loopback_message = "Loopback enabled".to_string();
                            } else {
                                loopback_status = "warn";
                                loopback_message = "Loopback set but readback is OFF".to_string();
                            }
                        }
                        Err(err) => {
                            loopback_status = "warn";
                            loopback_message = format!("Loopback set; readback failed: {}", err);
                        }
                    }
                }
                Err(err) => {
                    loopback_status = "warn";
                    loopback_message = format!("Loopback set failed: {}", err);
                }
            }
        }

        push_sanity_step(
            &mut steps,
            &app,
            "Loopback Setting",
            loopback_status,
            loopback_message,
            start,
        );

        // Loopback Echo test - send message and expect to receive our own echo
        // This should always pass if loopback is properly enabled
        if loopback_enabled {
            let start = Instant::now();
            let send_result = conn.send_message(options.arb_id, &options.data, options.extended);
            if let Err(err) = send_result {
                push_sanity_step(&mut steps, &app, "Loopback Echo", "fail", err, start);
            } else {
                *state.messages_sent.lock().unwrap() += 1;
                let mut received: Vec<CANMessage> = Vec::new();
                let deadline = Instant::now() + Duration::from_millis(500);
                let read_timeout = 100;
                let mut read_failed = false;

                while Instant::now() < deadline {
                    match conn.read_messages_with_loopback(read_timeout) {
                        Ok(mut msgs) => {
                            if !msgs.is_empty() {
                                received.append(&mut msgs);
                                break;
                            }
                        }
                        Err(err) => {
                            push_sanity_step(&mut steps, &app, "Loopback Echo", "fail", err, start);
                            received.clear();
                            read_failed = true;
                            break;
                        }
                    }
                }

                if read_failed {
                    // Failure already recorded
                } else if !received.is_empty() {
                    *state.messages_received.lock().unwrap() += received.len() as u64;
                    let first = &received[0];
                    let is_echo = first.arb_id == options.arb_id;
                    push_sanity_step(
                        &mut steps,
                        &app,
                        "Loopback Echo",
                        "pass",
                        format!(
                            "Echo received (ID 0x{:X}){}",
                            first.arb_id,
                            if is_echo { "" } else { " - ID mismatch" }
                        ),
                        start,
                    );
                } else {
                    push_sanity_step(
                        &mut steps,
                        &app,
                        "Loopback Echo",
                        "fail",
                        "No echo received despite loopback enabled".to_string(),
                        start,
                    );
                }
            }

            // Disable loopback for bus response test
            let _ = conn.set_loopback(false);
        } else {
            push_sanity_step(
                &mut steps,
                &app,
                "Loopback Echo",
                "skip",
                "Loopback not available".to_string(),
                Instant::now(),
            );
        }

        // Bus Response test - send message and check for real bus traffic
        // This may pass or warn depending on what's connected to the bus
        let start = Instant::now();
        let send_result = conn.send_message(options.arb_id, &options.data, options.extended);
        if let Err(err) = send_result {
            push_sanity_step(&mut steps, &app, "Bus Response", "fail", err, start);
        } else {
            *state.messages_sent.lock().unwrap() += 1;
            let mut received: Vec<CANMessage> = Vec::new();
            let deadline = Instant::now()
                + Duration::from_millis(options.response_timeout_ms.max(100) as u64);
            let read_timeout = options.response_timeout_ms.min(250).max(50);
            let mut read_failed = false;

            while Instant::now() < deadline {
                match conn.read_messages(read_timeout) {
                    Ok(mut msgs) => {
                        if !msgs.is_empty() {
                            received.append(&mut msgs);
                            break;
                        }
                    }
                    Err(err) => {
                        push_sanity_step(&mut steps, &app, "Bus Response", "fail", err, start);
                        received.clear();
                        read_failed = true;
                        break;
                    }
                }
            }

            if read_failed {
                // Failure already recorded
            } else if !received.is_empty() {
                *state.messages_received.lock().unwrap() += received.len() as u64;
                let first = &received[0];
                push_sanity_step(
                    &mut steps,
                    &app,
                    "Bus Response",
                    "pass",
                    format!(
                        "Received {} message(s), first ID 0x{:X}",
                        received.len(),
                        first.arb_id
                    ),
                    start,
                );
            } else {
                push_sanity_step(
                    &mut steps,
                    &app,
                    "Bus Response",
                    "warn",
                    "No response (bus quiet or no ECU present)".to_string(),
                    start,
                );
            }
        }

        // Periodic message functional test - verify messages actually appear in loopback
        // Use a different arb ID to distinguish from manual sends
        let periodic_test_id: u32 = 0x7FF;
        let periodic_test_data = [0xDE, 0xAD, 0xBE, 0xEF];

        if loopback_supported {
            let start = Instant::now();
            // Enable loopback for this test
            if conn.set_loopback(true).is_err() {
                push_sanity_step(
                    &mut steps,
                    &app,
                    "Periodic Message",
                    "skip",
                    "Could not enable loopback for test".to_string(),
                    start,
                );
            } else {
                // Clear buffers before test
                let _ = conn.clear_buffers();

                match conn.start_periodic_message(
                    periodic_test_id,
                    &periodic_test_data,
                    options.periodic_interval_ms.max(50),
                    false, // Use standard ID for test
                ) {
                    Ok(msg_id) => {
                        // Wait for several intervals to accumulate messages
                        let wait_time = options.periodic_interval_ms.max(50) * 4;
                        std::thread::sleep(Duration::from_millis(wait_time as u64));

                        // Stop periodic before reading to avoid race
                        let stop_result = conn.stop_periodic_message(msg_id);

                        // Read accumulated messages (with loopback to see our own periodic)
                        let mut periodic_count = 0;
                        let deadline = Instant::now() + Duration::from_millis(200);
                        while Instant::now() < deadline {
                            match conn.read_messages_with_loopback(50) {
                                Ok(msgs) => {
                                    for msg in &msgs {
                                        if msg.arb_id == periodic_test_id {
                                            periodic_count += 1;
                                        }
                                    }
                                    if msgs.is_empty() {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }

                        if stop_result.is_err() {
                            push_sanity_step(
                                &mut steps,
                                &app,
                                "Periodic Message",
                                "fail",
                                "Failed to stop periodic message".to_string(),
                                start,
                            );
                        } else if periodic_count >= 2 {
                            push_sanity_step(
                                &mut steps,
                                &app,
                                "Periodic Message",
                                "pass",
                                format!("Received {} periodic messages via loopback", periodic_count),
                                start,
                            );
                        } else if periodic_count == 1 {
                            push_sanity_step(
                                &mut steps,
                                &app,
                                "Periodic Message",
                                "warn",
                                "Only 1 periodic message received (timing issue?)".to_string(),
                                start,
                            );
                        } else {
                            push_sanity_step(
                                &mut steps,
                                &app,
                                "Periodic Message",
                                "fail",
                                "No periodic messages received despite loopback".to_string(),
                                start,
                            );
                        }
                    }
                    Err(err) => {
                        push_sanity_step(&mut steps, &app, "Periodic Message", "fail", err, start);
                    }
                }

                // Disable loopback after test
                let _ = conn.set_loopback(false);
            }
        } else {
            // Without loopback, just do basic API test
            let start = Instant::now();
            match conn.start_periodic_message(
                periodic_test_id,
                &periodic_test_data,
                options.periodic_interval_ms.max(50),
                false,
            ) {
                Ok(msg_id) => {
                    std::thread::sleep(Duration::from_millis(100));
                    match conn.stop_periodic_message(msg_id) {
                        Ok(()) => {
                            push_sanity_step(
                                &mut steps,
                                &app,
                                "Periodic Message",
                                "warn",
                                format!("API works (ID {}), but no loopback to verify delivery", msg_id),
                                start,
                            );
                        }
                        Err(err) => {
                            push_sanity_step(&mut steps, &app, "Periodic Message", "fail", err, start);
                        }
                    }
                }
                Err(err) => {
                    push_sanity_step(&mut steps, &app, "Periodic Message", "fail", err, start);
                }
            }
        }

        // Message filter functional test - verify filtering actually works
        // Test with two different IDs: one that should pass, one that should be blocked
        let filter_pass_id: u32 = 0x100;
        let filter_block_id: u32 = 0x200;
        let filter_test_data = [0x11, 0x22, 0x33, 0x44];

        if loopback_supported {
            let start = Instant::now();
            // Enable loopback for this test
            if conn.set_loopback(true).is_err() {
                push_sanity_step(
                    &mut steps,
                    &app,
                    "Message Filter",
                    "skip",
                    "Could not enable loopback for test".to_string(),
                    start,
                );
            } else {
                // Clear existing filters and buffers
                let _ = conn.clear_filters();
                let _ = conn.clear_buffers();

                // Add a pass filter that only allows filter_pass_id (0x100)
                // Mask 0x7FF means all 11 bits must match, pattern is the ID to match
                let full_mask = [0x00, 0x00, 0x07, 0xFF]; // Match all 11 bits of standard CAN ID
                let pattern = [(filter_pass_id >> 24) as u8, (filter_pass_id >> 16) as u8,
                              (filter_pass_id >> 8) as u8, filter_pass_id as u8];

                match conn.add_filter(PASS_FILTER, &full_mask, &pattern, false) {
                    Ok(filter_id) => {
                        // Send message that should pass filter
                        let _ = conn.send_message(filter_pass_id, &filter_test_data, false);
                        // Send message that should be blocked
                        let _ = conn.send_message(filter_block_id, &filter_test_data, false);

                        std::thread::sleep(Duration::from_millis(100));

                        // Read messages and check what we got
                        let mut pass_count = 0;
                        let mut block_count = 0;
                        let deadline = Instant::now() + Duration::from_millis(200);
                        while Instant::now() < deadline {
                            match conn.read_messages_with_loopback(50) {
                                Ok(msgs) => {
                                    for msg in &msgs {
                                        if msg.arb_id == filter_pass_id {
                                            pass_count += 1;
                                        } else if msg.arb_id == filter_block_id {
                                            block_count += 1;
                                        }
                                    }
                                    if msgs.is_empty() {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }

                        // Remove filter and restore pass-all
                        let _ = conn.remove_filter(filter_id);

                        // Evaluate results
                        if pass_count > 0 && block_count == 0 {
                            push_sanity_step(
                                &mut steps,
                                &app,
                                "Message Filter",
                                "pass",
                                format!("Filter working: {} passed, {} blocked", pass_count, block_count),
                                start,
                            );
                        } else if pass_count > 0 && block_count > 0 {
                            push_sanity_step(
                                &mut steps,
                                &app,
                                "Message Filter",
                                "warn",
                                format!("Filter partial: {} passed, {} leaked through", pass_count, block_count),
                                start,
                            );
                        } else if pass_count == 0 && block_count == 0 {
                            push_sanity_step(
                                &mut steps,
                                &app,
                                "Message Filter",
                                "warn",
                                "No messages received (filter may be blocking all or loopback issue)".to_string(),
                                start,
                            );
                        } else {
                            push_sanity_step(
                                &mut steps,
                                &app,
                                "Message Filter",
                                "fail",
                                format!("Filter inverted? {} passed (expected), {} blocked (unexpected)", pass_count, block_count),
                                start,
                            );
                        }
                    }
                    Err(err) => {
                        push_sanity_step(&mut steps, &app, "Message Filter", "fail", err, start);
                    }
                }

                // Restore pass-all filter and disable loopback
                let pass_all_mask = [0u8, 0u8, 0u8, 0u8];
                let pass_all_pattern = [0u8, 0u8, 0u8, 0u8];
                let _ = conn.add_filter(PASS_FILTER, &pass_all_mask, &pass_all_pattern, options.extended);
                let _ = conn.set_loopback(false);
            }
        } else {
            // Without loopback, just do basic API test
            let start = Instant::now();
            let mask = [0u8, 0u8, 0u8, 0u8];
            let pattern = [0u8, 0u8, 0u8, 0u8];
            match conn.add_filter(PASS_FILTER, &mask, &pattern, options.extended) {
                Ok(filter_id) => match conn.remove_filter(filter_id) {
                    Ok(()) => {
                        push_sanity_step(
                            &mut steps,
                            &app,
                            "Message Filter",
                            "warn",
                            format!("API works (ID {}), but no loopback to verify filtering", filter_id),
                            start,
                        );
                    }
                    Err(err) => {
                        push_sanity_step(&mut steps, &app, "Message Filter", "fail", err, start);
                    }
                },
                Err(err) => {
                    push_sanity_step(&mut steps, &app, "Message Filter", "fail", err, start);
                }
            }
        }

        // Restore loopback to original state if it was originally ON
        if loopback_supported && original_loopback == Some(true) {
            let start = Instant::now();
            match conn.set_loopback(true) {
                Ok(()) => {
                    push_sanity_step(
                        &mut steps,
                        &app,
                        "Restore Loopback",
                        "pass",
                        "Loopback restored to ON".to_string(),
                        start,
                    );
                }
                Err(err) => {
                    push_sanity_step(&mut steps, &app, "Restore Loopback", "warn", err, start);
                }
            }
        }

        let completed_at = now_millis();
        let duration_ms = completed_at.saturating_sub(started_at);

        Ok(SanityReport {
            started_at,
            completed_at,
            duration_ms,
            device_name,
            baud_rate,
            use_extended_id: options.extended,
            steps,
        })
    }

    #[cfg(not(windows))]
    {
        let _ = (options, state);
        Err("ERR_J2534_NOT_SUPPORTED: J2534 is only supported on Windows".to_string())
    }
}

#[tauri::command]
fn save_sanity_report(path: String, contents: String) -> Result<(), String> {
    std::fs::write(&path, contents)
        .map_err(|e| format!("ERR_SAVE_FAILED: {}", e))
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
            j2534_run_sanity_suite,
            save_sanity_report,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
