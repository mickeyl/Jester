//! IPC Protocol for J2534 Bridge
//!
//! Uses a simple JSON-RPC style protocol over named pipes.
//! Each message is a JSON object followed by a newline.

use serde::{Deserialize, Serialize};

/// Request from the main app to the bridge
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Request {
    /// List available J2534 devices
    EnumerateDevices,

    /// Open a connection to a device
    Open {
        dll_path: String,
        protocol_id: u32,
        baud_rate: u32,
        use_extended_id: bool,
    },

    /// Close the current connection
    Close,

    /// Send a CAN message
    SendMessage {
        arb_id: u32,
        data: Vec<u8>,
        extended: bool,
    },

    /// Send multiple CAN messages in a single PassThruWriteMsgs call
    /// Each message is (arb_id, data, extended)
    SendMessagesBatch {
        messages: Vec<BatchMessage>,
    },

    /// Read messages (with timeout in ms)
    ReadMessages { timeout_ms: u32 },

    /// Read messages including loopback echoes (for sanity testing)
    ReadMessagesWithLoopback { timeout_ms: u32 },

    /// Clear TX and RX buffers
    ClearBuffers,

    /// Read version information
    ReadVersion,

    /// Get last error string
    GetLastError,

    /// Read battery voltage
    ReadBatteryVoltage,

    /// Read programming voltage
    ReadProgrammingVoltage,

    /// Start a periodic message
    StartPeriodicMessage {
        arb_id: u32,
        data: Vec<u8>,
        interval_ms: u32,
        extended: bool,
    },

    /// Stop a periodic message
    StopPeriodicMessage { msg_id: u32 },

    /// Clear all periodic messages
    ClearPeriodicMessages,

    /// Add a message filter
    AddFilter {
        filter_type: String,
        mask: Vec<u8>,
        pattern: Vec<u8>,
        extended: bool,
    },

    /// Remove a message filter
    RemoveFilter { filter_id: u32 },

    /// Clear all filters
    ClearFilters,

    /// Get a configuration parameter
    GetConfig { parameter: u32 },

    /// Set a configuration parameter
    SetConfig { parameter: u32, value: u32 },

    /// Get loopback setting
    GetLoopback,

    /// Set loopback setting
    SetLoopback { enabled: bool },

    /// Get current data rate
    GetDataRate,

    /// Shutdown the bridge process
    Shutdown,
}

/// Response from the bridge to the main app
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum Response {
    /// Successful response with optional data
    #[serde(rename = "ok")]
    Ok { data: ResponseData },

    /// Error response
    #[serde(rename = "error")]
    Error { code: i32, message: String },
}

/// Data payload for successful responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseData {
    /// No data (for void operations)
    None,

    /// List of devices
    Devices(Vec<DeviceInfo>),

    /// Connection opened successfully
    Connected,

    /// Messages read from the bus
    Messages(Vec<CanMessage>),

    /// Version information
    Version(VersionInfo),

    /// String result (e.g., last error)
    String(String),

    /// Numeric result (e.g., voltage in mV, filter ID, msg ID)
    Number(u32),

    /// Float result (e.g., voltage in V)
    Float(f64),

    /// Boolean result
    Bool(bool),
}

/// J2534 device information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub name: String,
    pub vendor: String,
    pub dll_path: String,
    pub can_iso15765: bool,
    pub can_iso11898: bool,
    pub compatible: bool,
    pub bitness: u8, // 32 or 64
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

/// Message for batch sending
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchMessage {
    pub arb_id: u32,
    pub data: Vec<u8>,
    pub extended: bool,
}

/// Version information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub firmware_version: String,
    pub dll_version: String,
    pub api_version: String,
}

/// Progress update (sent asynchronously during connection)
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressUpdate {
    pub step: String,
    pub status: String,
    pub message: Option<String>,
}

/// Wrapper for messages that include an ID for request/response matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message<T> {
    pub id: u64,
    #[serde(flatten)]
    pub payload: T,
}

impl Response {
    pub fn ok(data: ResponseData) -> Self {
        Response::Ok { data }
    }

    pub fn ok_none() -> Self {
        Response::Ok {
            data: ResponseData::None,
        }
    }

    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Response::Error {
            code,
            message: message.into(),
        }
    }
}
