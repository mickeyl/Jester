//! Bridge Client
//!
//! Communicates with the j2534-bridge process via named pipes.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

#[cfg(windows)]
use std::os::windows::io::FromRawHandle;

/// Request types matching the bridge protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Request {
    EnumerateDevices,
    Open {
        dll_path: String,
        protocol_id: u32,
        baud_rate: u32,
        use_extended_id: bool,
    },
    Close,
    SendMessage {
        arb_id: u32,
        data: Vec<u8>,
        extended: bool,
    },
    ReadMessages {
        timeout_ms: u32,
    },
    ClearBuffers,
    ReadVersion,
    GetLastError,
    ReadBatteryVoltage,
    ReadProgrammingVoltage,
    StartPeriodicMessage {
        arb_id: u32,
        data: Vec<u8>,
        interval_ms: u32,
        extended: bool,
    },
    StopPeriodicMessage {
        msg_id: u32,
    },
    ClearPeriodicMessages,
    AddFilter {
        filter_type: String,
        mask: Vec<u8>,
        pattern: Vec<u8>,
        extended: bool,
    },
    RemoveFilter {
        filter_id: u32,
    },
    ClearFilters,
    GetConfig {
        parameter: u32,
    },
    SetConfig {
        parameter: u32,
        value: u32,
    },
    GetLoopback,
    SetLoopback {
        enabled: bool,
    },
    GetDataRate,
    Shutdown,
}

/// Response from the bridge
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum Response {
    #[serde(rename = "ok")]
    Ok { data: ResponseData },
    #[serde(rename = "error")]
    Error { code: i32, message: String },
}

/// Data payload for successful responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseData {
    None,
    Devices(Vec<DeviceInfo>),
    Connected,
    Messages(Vec<CanMessage>),
    Version(VersionInfo),
    String(String),
    Number(u32),
    Float(f64),
    Bool(bool),
}

/// Device information from the bridge
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub name: String,
    pub vendor: String,
    pub dll_path: String,
    pub can_iso15765: bool,
    pub can_iso11898: bool,
    pub compatible: bool,
    pub bitness: u8,
}

/// CAN message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanMessage {
    pub timestamp_us: u64,
    pub arb_id: u32,
    pub extended: bool,
    pub data: Vec<u8>,
}

/// Version information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub firmware_version: String,
    pub dll_version: String,
    pub api_version: String,
}

/// Message wrapper with ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message<T> {
    pub id: u64,
    #[serde(flatten)]
    pub payload: T,
}

/// Bridge client that manages the bridge process and communication
pub struct BridgeClient {
    process: Option<Child>,
    pipe_name: String,
    writer: Option<std::fs::File>,
    reader: Option<BufReader<std::fs::File>>,
    next_id: AtomicU64,
    #[allow(dead_code)]
    pending_responses: Mutex<std::collections::HashMap<u64, Response>>,
}

impl BridgeClient {
    /// Create a new bridge client (doesn't start the bridge yet)
    pub fn new() -> Self {
        let pipe_name = format!(
            "\\\\.\\pipe\\jester-j2534-{}",
            std::process::id()
        );

        Self {
            process: None,
            pipe_name,
            writer: None,
            reader: None,
            next_id: AtomicU64::new(1),
            pending_responses: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Get the path to the bridge executable for the given bitness
    fn get_bridge_path(bitness: u8) -> Result<std::path::PathBuf, String> {
        // Look for the bridge executable relative to the main app
        let exe_dir = std::env::current_exe()
            .map_err(|e| format!("Failed to get executable path: {}", e))?
            .parent()
            .ok_or("Failed to get executable directory")?
            .to_path_buf();

        let bridge_name = if bitness == 32 {
            "j2534-bridge-32.exe"
        } else {
            "j2534-bridge-64.exe"
        };

        // Try production location (same directory as main exe)
        let bridge_path = exe_dir.join(bridge_name);
        if bridge_path.exists() {
            return Ok(bridge_path);
        }

        // Try in the j2534-bridge target directory (for development)
        // exe_dir is like: Jester/src-tauri/target/x86_64-pc-windows-msvc/debug/
        // We need to go up 4 levels to get to Jester/
        let target_triple = if bitness == 32 {
            "i686-pc-windows-msvc"
        } else {
            "x86_64-pc-windows-msvc"
        };

        // Try release build first, then debug
        for build_type in &["release", "debug"] {
            let dev_path = exe_dir
                .parent()  // -> target/x86_64-pc-windows-msvc/
                .and_then(|p| p.parent())  // -> target/
                .and_then(|p| p.parent())  // -> src-tauri/
                .and_then(|p| p.parent())  // -> Jester/
                .map(|p| p.join("j2534-bridge")
                          .join("target")
                          .join(target_triple)
                          .join(build_type)
                          .join("j2534-bridge.exe"));

            if let Some(path) = dev_path {
                eprintln!("[client] Looking for bridge at: {:?}", path);
                if path.exists() {
                    return Ok(path);
                }
            }
        }

        Err(format!("Bridge executable not found: {} (looked in {:?} and development paths)", bridge_name, exe_dir))
    }

    /// Start the bridge process for the given DLL bitness
    #[cfg(windows)]
    pub fn start(&mut self, bitness: u8) -> Result<(), String> {
        use windows::core::PCSTR;
        use windows::Win32::Storage::FileSystem::{
            CreateFileA, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_NONE, OPEN_EXISTING,
        };

        if self.process.is_some() {
            return Err("Bridge already running".into());
        }

        let bridge_path = Self::get_bridge_path(bitness)?;
        eprintln!("[client] Starting bridge: {:?}", bridge_path);
        eprintln!("[client] Pipe name: {}", self.pipe_name);

        // Start the bridge process
        let child = Command::new(&bridge_path)
            .arg(&self.pipe_name)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("Failed to start bridge: {}", e))?;

        self.process = Some(child);

        // Give the bridge time to create the pipe
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Connect to the named pipe
        let pipe_name_cstr = std::ffi::CString::new(self.pipe_name.as_str())
            .map_err(|e| format!("Invalid pipe name: {}", e))?;

        let pipe_handle = unsafe {
            CreateFileA(
                PCSTR::from_raw(pipe_name_cstr.as_ptr() as *const u8),
                (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }.map_err(|e| format!("Failed to connect to pipe: {}", e))?;

        // Convert to std::fs::File
        let handle_raw = pipe_handle.0 as *mut std::ffi::c_void;
        let file = unsafe { std::fs::File::from_raw_handle(handle_raw) };

        let reader_file = file.try_clone()
            .map_err(|e| format!("Failed to clone pipe handle: {}", e))?;

        self.writer = Some(file);
        self.reader = Some(BufReader::new(reader_file));

        eprintln!("[client] Connected to bridge");
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn start(&mut self, _bitness: u8) -> Result<(), String> {
        Err("Bridge client is only supported on Windows".into())
    }

    /// Stop the bridge process
    pub fn stop(&mut self) -> Result<(), String> {
        if self.writer.is_some() {
            // Send shutdown request
            let _ = self.send_request(Request::Shutdown);
        }

        self.writer = None;
        self.reader = None;

        if let Some(mut child) = self.process.take() {
            // Give it a moment to exit gracefully
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = child.kill();
            let _ = child.wait();
        }

        Ok(())
    }

    /// Send a request and wait for the response
    pub fn send_request(&mut self, request: Request) -> Result<Response, String> {
        let writer = self.writer.as_mut()
            .ok_or("Bridge not connected")?;
        let reader = self.reader.as_mut()
            .ok_or("Bridge not connected")?;

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = Message {
            id,
            payload: request,
        };

        let json = serde_json::to_string(&msg)
            .map_err(|e| format!("Failed to serialize request: {}", e))?;

        writeln!(writer, "{}", json)
            .map_err(|e| format!("Failed to write to pipe: {}", e))?;
        writer.flush()
            .map_err(|e| format!("Failed to flush pipe: {}", e))?;

        // Read response
        let mut line = String::new();
        reader.read_line(&mut line)
            .map_err(|e| format!("Failed to read from pipe: {}", e))?;

        let response: Message<Response> = serde_json::from_str(&line)
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if response.id != id {
            return Err(format!("Response ID mismatch: expected {}, got {}", id, response.id));
        }

        Ok(response.payload)
    }

    /// Check if the bridge is running
    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        self.process.is_some() && self.writer.is_some()
    }
}

impl Drop for BridgeClient {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl Default for BridgeClient {
    fn default() -> Self {
        Self::new()
    }
}
