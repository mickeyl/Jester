// This module contains direct J2534 DLL loading support.
// Currently unused since the unified bridge approach is used instead,
// but kept as a reference implementation for native 64-bit DLLs.
#![allow(dead_code)]

use libloading::{Library, Symbol};
use serde::{Deserialize, Serialize};
use std::ffi::{c_char, c_ulong, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use winreg::enums::*;
use winreg::RegKey;

// J2534 Protocol IDs
// Note: Only CAN protocol is supported. Other protocols (ISO15765, ISO9141, etc.)
// are optional in the J2534 spec, so adapter support is inconsistent.
#[allow(dead_code)]
pub const PROTOCOL_J1850VPW: u32 = 1;
#[allow(dead_code)]
pub const PROTOCOL_J1850PWM: u32 = 2;
#[allow(dead_code)]
pub const PROTOCOL_ISO9141: u32 = 3;
#[allow(dead_code)]
pub const PROTOCOL_ISO14230: u32 = 4;
pub const PROTOCOL_CAN: u32 = 5;
#[allow(dead_code)]
pub const PROTOCOL_ISO15765: u32 = 6;
#[allow(dead_code)]
pub const PROTOCOL_SCI_A_ENGINE: u32 = 7;
#[allow(dead_code)]
pub const PROTOCOL_SCI_A_TRANS: u32 = 8;
#[allow(dead_code)]
pub const PROTOCOL_SCI_B_ENGINE: u32 = 9;
#[allow(dead_code)]
pub const PROTOCOL_SCI_B_TRANS: u32 = 10;

// J2534 Connect Flags
pub const CAN_29BIT_ID: u32 = 0x100;
#[allow(dead_code)]
pub const ISO9141_NO_CHECKSUM: u32 = 0x200;
#[allow(dead_code)]
pub const CAN_ID_BOTH: u32 = 0x800;
#[allow(dead_code)]
pub const ISO9141_K_LINE_ONLY: u32 = 0x1000;

// J2534 Filter Types
pub const PASS_FILTER: u32 = 1;
#[allow(dead_code)]
pub const BLOCK_FILTER: u32 = 2;
#[allow(dead_code)]
pub const FLOW_CONTROL_FILTER: u32 = 3;

// J2534 IOCTL IDs
pub const GET_CONFIG: u32 = 0x01;
pub const SET_CONFIG: u32 = 0x02;
pub const READ_VBATT: u32 = 0x03;
#[allow(dead_code)]
pub const FIVE_BAUD_INIT: u32 = 0x04;
#[allow(dead_code)]
pub const FAST_INIT: u32 = 0x05;
pub const CLEAR_TX_BUFFER: u32 = 0x07;
pub const CLEAR_RX_BUFFER: u32 = 0x08;
pub const CLEAR_PERIODIC_MSGS: u32 = 0x09;
pub const CLEAR_MSG_FILTERS: u32 = 0x0A;
#[allow(dead_code)]
pub const CLEAR_FUNCT_MSG_LOOKUP_TABLE: u32 = 0x0B;
#[allow(dead_code)]
pub const ADD_TO_FUNCT_MSG_LOOKUP_TABLE: u32 = 0x0C;
#[allow(dead_code)]
pub const DELETE_FROM_FUNCT_MSG_LOOKUP_TABLE: u32 = 0x0D;
pub const READ_PROG_VOLTAGE: u32 = 0x0E;

// J2534 Config Parameter IDs
pub const DATA_RATE: u32 = 0x01;
pub const LOOPBACK: u32 = 0x03;
#[allow(dead_code)]
pub const NODE_ADDRESS: u32 = 0x04;
#[allow(dead_code)]
pub const NETWORK_LINE: u32 = 0x05;
#[allow(dead_code)]
pub const P1_MIN: u32 = 0x06;
#[allow(dead_code)]
pub const P1_MAX: u32 = 0x07;
#[allow(dead_code)]
pub const P2_MIN: u32 = 0x08;
#[allow(dead_code)]
pub const P2_MAX: u32 = 0x09;
#[allow(dead_code)]
pub const P3_MIN: u32 = 0x0A;
#[allow(dead_code)]
pub const P3_MAX: u32 = 0x0B;
#[allow(dead_code)]
pub const P4_MIN: u32 = 0x0C;
#[allow(dead_code)]
pub const P4_MAX: u32 = 0x0D;
#[allow(dead_code)]
pub const W1: u32 = 0x0E;
#[allow(dead_code)]
pub const W2: u32 = 0x0F;
#[allow(dead_code)]
pub const W3: u32 = 0x10;
#[allow(dead_code)]
pub const W4: u32 = 0x11;
#[allow(dead_code)]
pub const W5: u32 = 0x12;
#[allow(dead_code)]
pub const TIDLE: u32 = 0x13;
#[allow(dead_code)]
pub const TINIL: u32 = 0x14;
#[allow(dead_code)]
pub const TWUP: u32 = 0x15;
#[allow(dead_code)]
pub const PARITY: u32 = 0x16;
#[allow(dead_code)]
pub const BIT_SAMPLE_POINT: u32 = 0x17;
#[allow(dead_code)]
pub const SYNC_JUMP_WIDTH: u32 = 0x18;
#[allow(dead_code)]
pub const W0: u32 = 0x19;
#[allow(dead_code)]
pub const T1_MAX: u32 = 0x1A;
#[allow(dead_code)]
pub const T2_MAX: u32 = 0x1B;
#[allow(dead_code)]
pub const T4_MAX: u32 = 0x1C;
#[allow(dead_code)]
pub const T5_MAX: u32 = 0x1D;
#[allow(dead_code)]
pub const ISO15765_BS: u32 = 0x1E;
#[allow(dead_code)]
pub const ISO15765_STMIN: u32 = 0x1F;
#[allow(dead_code)]
pub const DATA_BITS: u32 = 0x20;
#[allow(dead_code)]
pub const FIVE_BAUD_MOD: u32 = 0x21;
#[allow(dead_code)]
pub const BS_TX: u32 = 0x22;
#[allow(dead_code)]
pub const STMIN_TX: u32 = 0x23;
#[allow(dead_code)]
pub const T3_MAX: u32 = 0x24;
#[allow(dead_code)]
pub const ISO15765_WFT_MAX: u32 = 0x25;

// J2534 Error codes
pub const STATUS_NOERROR: i32 = 0x00;
pub const ERR_NOT_SUPPORTED: i32 = 0x01;
pub const ERR_INVALID_CHANNEL_ID: i32 = 0x02;
pub const ERR_INVALID_PROTOCOL_ID: i32 = 0x03;
pub const ERR_NULL_PARAMETER: i32 = 0x04;
pub const ERR_INVALID_IOCTL_VALUE: i32 = 0x05;
pub const ERR_INVALID_FLAGS: i32 = 0x06;
pub const ERR_FAILED: i32 = 0x07;
pub const ERR_DEVICE_NOT_CONNECTED: i32 = 0x08;
pub const ERR_TIMEOUT: i32 = 0x09;
pub const ERR_INVALID_MSG: i32 = 0x0A;
pub const ERR_INVALID_TIME_INTERVAL: i32 = 0x0B;
pub const ERR_EXCEEDED_LIMIT: i32 = 0x0C;
pub const ERR_INVALID_MSG_ID: i32 = 0x0D;
pub const ERR_DEVICE_IN_USE: i32 = 0x0E;
pub const ERR_INVALID_IOCTL_ID: i32 = 0x0F;
pub const ERR_BUFFER_EMPTY: i32 = 0x10;
pub const ERR_BUFFER_FULL: i32 = 0x11;
pub const ERR_BUFFER_OVERFLOW: i32 = 0x12;
pub const ERR_PIN_INVALID: i32 = 0x13;
pub const ERR_CHANNEL_IN_USE: i32 = 0x14;
pub const ERR_MSG_PROTOCOL_ID: i32 = 0x15;
pub const ERR_INVALID_FILTER_ID: i32 = 0x16;
pub const ERR_NO_FLOW_CONTROL: i32 = 0x17;
pub const ERR_NOT_UNIQUE: i32 = 0x18;
pub const ERR_INVALID_BAUDRATE: i32 = 0x19;
pub const ERR_INVALID_DEVICE_ID: i32 = 0x1A;

// RX Status flags
pub const RX_CAN_29BIT_ID: u32 = 0x100;
pub const TX_MSG_TYPE: u32 = 0x01;

/// Get error code description
pub fn error_code_to_string(code: i32) -> &'static str {
    match code {
        STATUS_NOERROR => "STATUS_NOERROR",
        ERR_NOT_SUPPORTED => "ERR_NOT_SUPPORTED",
        ERR_INVALID_CHANNEL_ID => "ERR_INVALID_CHANNEL_ID",
        ERR_INVALID_PROTOCOL_ID => "ERR_INVALID_PROTOCOL_ID",
        ERR_NULL_PARAMETER => "ERR_NULL_PARAMETER",
        ERR_INVALID_IOCTL_VALUE => "ERR_INVALID_IOCTL_VALUE",
        ERR_INVALID_FLAGS => "ERR_INVALID_FLAGS",
        ERR_FAILED => "ERR_FAILED",
        ERR_DEVICE_NOT_CONNECTED => "ERR_DEVICE_NOT_CONNECTED",
        ERR_TIMEOUT => "ERR_TIMEOUT",
        ERR_INVALID_MSG => "ERR_INVALID_MSG",
        ERR_INVALID_TIME_INTERVAL => "ERR_INVALID_TIME_INTERVAL",
        ERR_EXCEEDED_LIMIT => "ERR_EXCEEDED_LIMIT",
        ERR_INVALID_MSG_ID => "ERR_INVALID_MSG_ID",
        ERR_DEVICE_IN_USE => "ERR_DEVICE_IN_USE",
        ERR_INVALID_IOCTL_ID => "ERR_INVALID_IOCTL_ID",
        ERR_BUFFER_EMPTY => "ERR_BUFFER_EMPTY",
        ERR_BUFFER_FULL => "ERR_BUFFER_FULL",
        ERR_BUFFER_OVERFLOW => "ERR_BUFFER_OVERFLOW",
        ERR_PIN_INVALID => "ERR_PIN_INVALID",
        ERR_CHANNEL_IN_USE => "ERR_CHANNEL_IN_USE",
        ERR_MSG_PROTOCOL_ID => "ERR_MSG_PROTOCOL_ID",
        ERR_INVALID_FILTER_ID => "ERR_INVALID_FILTER_ID",
        ERR_NO_FLOW_CONTROL => "ERR_NO_FLOW_CONTROL",
        ERR_NOT_UNIQUE => "ERR_NOT_UNIQUE",
        ERR_INVALID_BAUDRATE => "ERR_INVALID_BAUDRATE",
        ERR_INVALID_DEVICE_ID => "ERR_INVALID_DEVICE_ID",
        _ => "UNKNOWN_ERROR",
    }
}

/// Get protocol name from ID
#[allow(dead_code)]
pub fn protocol_id_to_string(id: u32) -> &'static str {
    match id {
        PROTOCOL_J1850VPW => "J1850VPW",
        PROTOCOL_J1850PWM => "J1850PWM",
        PROTOCOL_ISO9141 => "ISO9141",
        PROTOCOL_ISO14230 => "ISO14230",
        PROTOCOL_CAN => "CAN",
        PROTOCOL_ISO15765 => "ISO15765",
        PROTOCOL_SCI_A_ENGINE => "SCI_A_ENGINE",
        PROTOCOL_SCI_A_TRANS => "SCI_A_TRANS",
        PROTOCOL_SCI_B_ENGINE => "SCI_B_ENGINE",
        PROTOCOL_SCI_B_TRANS => "SCI_B_TRANS",
        _ => "UNKNOWN",
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct PassThruMsg {
    pub protocol_id: u32,
    pub rx_status: u32,
    pub tx_flags: u32,
    pub timestamp: u32,
    pub data_size: u32,
    pub extra_data_index: u32,
    pub data: [u8; 4128],
}

impl Default for PassThruMsg {
    fn default() -> Self {
        Self {
            protocol_id: 0,
            rx_status: 0,
            tx_flags: 0,
            timestamp: 0,
            data_size: 0,
            extra_data_index: 0,
            data: [0u8; 4128],
        }
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534Config {
    pub device_name: String,
    pub baud_rate: u32,
    pub protocol: String,
    pub use_extended_id: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CANMessage {
    pub timestamp_us: u64,
    pub arb_id: u32,
    pub extended: bool,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534Progress {
    pub step: String,
    pub status: String, // "pending" | "in_progress" | "success" | "error"
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub connected: bool,
    pub device_name: String,
    pub baud_rate: u32,
    pub messages_sent: u64,
    pub messages_received: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534VersionInfo {
    pub firmware_version: String,
    pub dll_version: String,
    pub api_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534VoltageInfo {
    pub battery_voltage: f64,
    pub programming_voltage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534TestResult {
    pub function_name: String,
    pub success: bool,
    pub error_code: i32,
    pub error_name: String,
    pub message: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534PeriodicMsg {
    pub msg_id: u32,
    pub arb_id: u32,
    pub data: Vec<u8>,
    pub interval_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534FilterInfo {
    pub filter_id: u32,
    pub filter_type: u32,
    pub mask: Vec<u8>,
    pub pattern: Vec<u8>,
}

/// SCONFIG structure for GET_CONFIG/SET_CONFIG
#[repr(C)]
#[derive(Debug, Clone)]
pub struct SConfig {
    pub parameter: u32,
    pub value: u32,
}

/// SCONFIG_LIST structure
#[repr(C)]
#[derive(Debug)]
pub struct SConfigList {
    pub num_of_params: u32,
    pub config_ptr: *mut SConfig,
}

#[allow(dead_code)]
pub struct J2534State {
    pub connection_active: Arc<AtomicBool>,
    pub device_name: Arc<Mutex<String>>,
    pub baud_rate: Arc<Mutex<u32>>,
    pub messages_sent: Arc<Mutex<u64>>,
    pub messages_received: Arc<Mutex<u64>>,
    pub device_id: Arc<Mutex<Option<u32>>>,
    pub channel_id: Arc<Mutex<Option<u32>>>,
    pub library: Arc<Mutex<Option<Library>>>,
}

impl Default for J2534State {
    fn default() -> Self {
        Self {
            connection_active: Arc::new(AtomicBool::new(false)),
            device_name: Arc::new(Mutex::new(String::new())),
            baud_rate: Arc::new(Mutex::new(500000)),
            messages_sent: Arc::new(Mutex::new(0)),
            messages_received: Arc::new(Mutex::new(0)),
            device_id: Arc::new(Mutex::new(None)),
            channel_id: Arc::new(Mutex::new(None)),
            library: Arc::new(Mutex::new(None)),
        }
    }
}

impl J2534State {
    #[allow(dead_code)]
    pub fn get_status(&self) -> ConnectionStatus {
        ConnectionStatus {
            connected: self.connection_active.load(Ordering::SeqCst),
            device_name: self.device_name.lock().unwrap().clone(),
            baud_rate: *self.baud_rate.lock().unwrap(),
            messages_sent: *self.messages_sent.lock().unwrap(),
            messages_received: *self.messages_received.lock().unwrap(),
        }
    }
}

// Check if the DLL matches the current process bitness
fn is_dll_compatible(dll_path: &str) -> bool {
    use std::fs::File;
    use std::io::Read;

    let mut file = match File::open(dll_path) {
        Ok(f) => f,
        Err(_) => return false,
    };

    // Read DOS header
    let mut dos_header = [0u8; 64];
    if file.read_exact(&mut dos_header).is_err() {
        return false;
    }

    // Check MZ signature
    if dos_header[0] != b'M' || dos_header[1] != b'Z' {
        return false;
    }

    // Get PE header offset
    let pe_offset = u32::from_le_bytes([dos_header[60], dos_header[61], dos_header[62], dos_header[63]]) as usize;

    // Seek to PE header
    let mut pe_header = vec![0u8; pe_offset + 6];
    let mut file = match File::open(dll_path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    if file.read_exact(&mut pe_header).is_err() {
        return false;
    }

    // Check PE signature
    if pe_header[pe_offset] != b'P' || pe_header[pe_offset + 1] != b'E' {
        return false;
    }

    // Get machine type
    let machine = u16::from_le_bytes([pe_header[pe_offset + 4], pe_header[pe_offset + 5]]);

    // 0x8664 = AMD64 (64-bit), 0x014c = i386 (32-bit)
    #[cfg(target_pointer_width = "64")]
    {
        machine == 0x8664
    }
    #[cfg(target_pointer_width = "32")]
    {
        machine == 0x014c
    }
}

pub fn enumerate_devices() -> Vec<J2534Device> {
    let mut devices = Vec::new();

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    // Check both 32-bit and 64-bit registry locations
    let registry_paths = [
        ("SOFTWARE\\PassThruSupport.04.04", KEY_READ | KEY_WOW64_64KEY),
        ("SOFTWARE\\PassThruSupport.04.04", KEY_READ | KEY_WOW64_32KEY),
    ];

    for (path, flags) in &registry_paths {
        if let Ok(passthru_key) = hklm.open_subkey_with_flags(path, *flags) {
            for device_name in passthru_key.enum_keys().filter_map(|k| k.ok()) {
                if let Ok(device_key) = passthru_key.open_subkey_with_flags(&device_name, *flags) {
                    let name: String = device_key.get_value("Name").unwrap_or(device_name.clone());
                    let vendor: String = device_key.get_value("Vendor").unwrap_or_default();
                    let dll_path: String = device_key.get_value("FunctionLibrary").unwrap_or_default();
                    let can_iso15765: u32 = device_key.get_value("ISO15765").unwrap_or(0);
                    let can_iso11898: u32 = device_key.get_value("CAN").unwrap_or(0);

                    if dll_path.is_empty() {
                        continue;
                    }

                    // Check if we already have this device (from the other registry view)
                    if devices.iter().any(|d: &J2534Device| d.dll_path == dll_path) {
                        continue;
                    }

                    let compatible = is_dll_compatible(&dll_path);

                    devices.push(J2534Device {
                        name: if compatible {
                            name
                        } else {
                            format!("{} (incompatible)", name)
                        },
                        vendor,
                        dll_path,
                        can_iso15765: can_iso15765 != 0,
                        can_iso11898: can_iso11898 != 0,
                        compatible,
                    });
                }
            }
        }
    }

    devices
}

// Use "system" calling convention which is stdcall on Windows and C on other platforms
type PassThruOpenFn = unsafe extern "system" fn(*const c_void, *mut c_ulong) -> i32;
type PassThruCloseFn = unsafe extern "system" fn(c_ulong) -> i32;
type PassThruConnectFn = unsafe extern "system" fn(c_ulong, c_ulong, c_ulong, c_ulong, *mut c_ulong) -> i32;
type PassThruDisconnectFn = unsafe extern "system" fn(c_ulong) -> i32;
type PassThruReadMsgsFn = unsafe extern "system" fn(c_ulong, *mut PassThruMsg, *mut c_ulong, c_ulong) -> i32;
type PassThruWriteMsgsFn = unsafe extern "system" fn(c_ulong, *mut PassThruMsg, *mut c_ulong, c_ulong) -> i32;
type PassThruStartMsgFilterFn = unsafe extern "system" fn(c_ulong, c_ulong, *const PassThruMsg, *const PassThruMsg, *const PassThruMsg, *mut c_ulong) -> i32;
type PassThruStopMsgFilterFn = unsafe extern "system" fn(c_ulong, c_ulong) -> i32;
type PassThruStartPeriodicMsgFn = unsafe extern "system" fn(c_ulong, *const PassThruMsg, *mut c_ulong, c_ulong) -> i32;
type PassThruStopPeriodicMsgFn = unsafe extern "system" fn(c_ulong, c_ulong) -> i32;
type PassThruIoctlFn = unsafe extern "system" fn(c_ulong, c_ulong, *mut c_void, *mut c_void) -> i32;
type PassThruReadVersionFn = unsafe extern "system" fn(c_ulong, *mut c_char, *mut c_char, *mut c_char) -> i32;
type PassThruGetLastErrorFn = unsafe extern "system" fn(*mut c_char) -> i32;
#[allow(dead_code)]
type PassThruSetProgrammingVoltageFn = unsafe extern "system" fn(c_ulong, c_ulong, c_ulong) -> i32;

/// Tracks a recently sent message for TX echo filtering
#[derive(Clone)]
struct SentMessage {
    arb_id: u32,
    data: Vec<u8>,
    timestamp: std::time::Instant,
}

pub struct J2534Connection {
    library: Library,
    device_id: u32,
    channel_id: u32,
    /// Recently sent messages for filtering TX echoes (driver workaround)
    sent_messages: std::sync::Mutex<Vec<SentMessage>>,
}

impl J2534Connection {
    pub fn open(dll_path: &str, baud_rate: u32, use_extended_id: bool, progress_callback: impl Fn(J2534Progress)) -> Result<Self, String> {
        // Load the DLL
        progress_callback(J2534Progress {
            step: "load_dll".to_string(),
            status: "in_progress".to_string(),
            message: Some(dll_path.to_string()),
        });

        let library = unsafe { Library::new(dll_path) }
            .map_err(|e| format!("ERR_J2534_DLL_LOAD: {}", e))?;

        progress_callback(J2534Progress {
            step: "load_dll".to_string(),
            status: "success".to_string(),
            message: None,
        });

        // Open the device
        progress_callback(J2534Progress {
            step: "open_device".to_string(),
            status: "in_progress".to_string(),
            message: None,
        });

        let mut device_id: c_ulong = 0;
        unsafe {
            let open_fn: Symbol<PassThruOpenFn> = library
                .get(b"PassThruOpen\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruOpen - {}", e))?;

            let result = open_fn(std::ptr::null(), &mut device_id);
            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_OPEN_FAILED: error code {}", result));
            }
        }

        progress_callback(J2534Progress {
            step: "open_device".to_string(),
            status: "success".to_string(),
            message: Some(format!("Device ID: {}", device_id)),
        });

        // Connect to CAN channel
        progress_callback(J2534Progress {
            step: "connect_channel".to_string(),
            status: "in_progress".to_string(),
            message: Some(format!("Baud rate: {} bps", baud_rate)),
        });

        let mut channel_id: c_ulong = 0;
        let flags = if use_extended_id { CAN_29BIT_ID } else { 0 };

        unsafe {
            let connect_fn: Symbol<PassThruConnectFn> = library
                .get(b"PassThruConnect\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruConnect - {}", e))?;

            let result = connect_fn(device_id, PROTOCOL_CAN as c_ulong, flags as c_ulong, baud_rate as c_ulong, &mut channel_id);
            if result != STATUS_NOERROR {
                // Clean up device on failure
                if let Ok(close_fn) = library.get::<PassThruCloseFn>(b"PassThruClose\0") {
                    close_fn(device_id);
                }
                return Err(format!("ERR_J2534_CONNECT_FAILED: error code {}", result));
            }
        }

        progress_callback(J2534Progress {
            step: "connect_channel".to_string(),
            status: "success".to_string(),
            message: Some(format!("Channel ID: {}", channel_id)),
        });

        // Set up pass-all filter
        progress_callback(J2534Progress {
            step: "set_filter".to_string(),
            status: "in_progress".to_string(),
            message: None,
        });

        unsafe {
            let filter_fn: Symbol<PassThruStartMsgFilterFn> = library
                .get(b"PassThruStartMsgFilter\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruStartMsgFilter - {}", e))?;

            let mut mask_msg = PassThruMsg::default();
            mask_msg.protocol_id = PROTOCOL_CAN;
            mask_msg.data_size = 4;
            // All zeros = match everything

            let mut pattern_msg = PassThruMsg::default();
            pattern_msg.protocol_id = PROTOCOL_CAN;
            pattern_msg.data_size = 4;
            // All zeros = match everything

            let mut filter_id: c_ulong = 0;
            let result = filter_fn(channel_id, PASS_FILTER, &mask_msg, &pattern_msg, std::ptr::null(), &mut filter_id);
            if result != STATUS_NOERROR {
                // Clean up on failure
                if let Ok(disconnect_fn) = library.get::<PassThruDisconnectFn>(b"PassThruDisconnect\0") {
                    disconnect_fn(channel_id);
                }
                if let Ok(close_fn) = library.get::<PassThruCloseFn>(b"PassThruClose\0") {
                    close_fn(device_id);
                }
                return Err(format!("ERR_J2534_FILTER_FAILED: error code {}", result));
            }
        }

        progress_callback(J2534Progress {
            step: "set_filter".to_string(),
            status: "success".to_string(),
            message: None,
        });

        // Disable loopback by default to prevent TX echo
        let conn = Self {
            library,
            device_id,
            channel_id,
            sent_messages: std::sync::Mutex::new(Vec::new()),
        };

        progress_callback(J2534Progress {
            step: "loopback".to_string(),
            status: "in_progress".to_string(),
            message: Some("Disabling loopback (if supported)".to_string()),
        });

        let set_result = conn.set_loopback(false);
        let readback = conn.get_loopback();
        let message = match (set_result, readback) {
            (Ok(()), Ok(state)) => Some(format!("Loopback reported {}", if state { "ON" } else { "OFF" })),
            (Ok(()), Err(err)) => Some(format!("Loopback set ok; readback failed: {}", err)),
            (Err(err), Ok(state)) => Some(format!("Loopback set failed: {}; device reports {}", err, if state { "ON" } else { "OFF" })),
            (Err(err), Err(err2)) => Some(format!("Loopback set failed: {}; readback failed: {}", err, err2)),
        };

        progress_callback(J2534Progress {
            step: "loopback".to_string(),
            status: "success".to_string(),
            message,
        });

        progress_callback(J2534Progress {
            step: "complete".to_string(),
            status: "success".to_string(),
            message: Some("Connection established".to_string()),
        });

        Ok(conn)
    }

    pub fn send_message(&self, arb_id: u32, data: &[u8], extended: bool) -> Result<(), String> {
        let mut msg = PassThruMsg::default();
        msg.protocol_id = PROTOCOL_CAN;
        msg.tx_flags = if extended { CAN_29BIT_ID } else { 0 };

        // First 4 bytes are the CAN ID
        msg.data[0] = ((arb_id >> 24) & 0xFF) as u8;
        msg.data[1] = ((arb_id >> 16) & 0xFF) as u8;
        msg.data[2] = ((arb_id >> 8) & 0xFF) as u8;
        msg.data[3] = (arb_id & 0xFF) as u8;

        // Copy data bytes
        let data_len = data.len().min(8);
        msg.data[4..4 + data_len].copy_from_slice(&data[..data_len]);
        msg.data_size = (4 + data_len) as u32;

        let mut num_msgs: c_ulong = 1;

        unsafe {
            let write_fn: Symbol<PassThruWriteMsgsFn> = self.library
                .get(b"PassThruWriteMsgs\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruWriteMsgs - {}", e))?;

            let result = write_fn(self.channel_id, &mut msg, &mut num_msgs, 1000);
            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_WRITE_FAILED: error code {} [{}]", result, error_code_to_string(result)));
            }
        }

        // Track sent message for TX echo filtering (driver workaround)
        // Some drivers don't properly set TX_MSG_TYPE flag on echoed messages
        if let Ok(mut sent) = self.sent_messages.lock() {
            // Clean up old entries (older than 500ms)
            let cutoff = std::time::Instant::now() - std::time::Duration::from_millis(500);
            sent.retain(|m| m.timestamp > cutoff);

            // Add this message
            sent.push(SentMessage {
                arb_id,
                data: data[..data_len].to_vec(),
                timestamp: std::time::Instant::now(),
            });
        }

        Ok(())
    }

    pub fn read_messages(&self, timeout_ms: u32) -> Result<Vec<CANMessage>, String> {
        self.read_messages_inner(timeout_ms, false)
    }

    pub fn read_messages_with_loopback(&self, timeout_ms: u32) -> Result<Vec<CANMessage>, String> {
        self.read_messages_inner(timeout_ms, true)
    }

    fn read_messages_inner(&self, timeout_ms: u32, include_loopback: bool) -> Result<Vec<CANMessage>, String> {
        let mut messages = Vec::new();

        // Create message buffer - use Vec since PassThruMsg is large and doesn't implement Copy
        let mut msg_buffer: Vec<PassThruMsg> = (0..16).map(|_| PassThruMsg::default()).collect();
        let mut num_msgs: c_ulong = 16;

        unsafe {
            let read_fn: Symbol<PassThruReadMsgsFn> = self.library
                .get(b"PassThruReadMsgs\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruReadMsgs - {}", e))?;

            let result = read_fn(self.channel_id, msg_buffer.as_mut_ptr(), &mut num_msgs, timeout_ms);

            // Allow STATUS_NOERROR, ERR_BUFFER_EMPTY, and ERR_TIMEOUT
            // ERR_TIMEOUT can still return messages (e.g., ScanDoc WiFi adapter)
            if result != STATUS_NOERROR && result != ERR_BUFFER_EMPTY && result != ERR_TIMEOUT {
                return Err(format!("ERR_J2534_READ_FAILED: error code {}", result));
            }
        }

        let base_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);

        for i in 0..num_msgs as usize {
            let msg = &msg_buffer[i];

            // Skip TX echo messages (loopback) - these have TX_MSG_TYPE flag set
            // Unless include_loopback is true (for sanity testing)
            if !include_loopback && (msg.rx_status & TX_MSG_TYPE) != 0 {
                continue;
            }

            if msg.data_size >= 4 {
                let arb_id = ((msg.data[0] as u32) << 24)
                    | ((msg.data[1] as u32) << 16)
                    | ((msg.data[2] as u32) << 8)
                    | (msg.data[3] as u32);

                let data_len = (msg.data_size - 4) as usize;
                let data = msg.data[4..4 + data_len].to_vec();
                let extended = (msg.rx_status & RX_CAN_29BIT_ID) != 0;

                // Driver workaround: filter out TX echoes by matching against recently sent messages
                // Some drivers don't set TX_MSG_TYPE flag even when loopback is enabled
                // Skip this filter if include_loopback is true
                let is_tx_echo = if include_loopback {
                    false
                } else if let Ok(mut sent) = self.sent_messages.lock() {
                    // Clean up old entries while we have the lock
                    let cutoff = std::time::Instant::now() - std::time::Duration::from_millis(500);
                    sent.retain(|m| m.timestamp > cutoff);

                    // Check if this message matches any recently sent message
                    if let Some(pos) = sent.iter().position(|m| m.arb_id == arb_id && m.data == data) {
                        // Remove the matched entry so we only filter once per send
                        sent.remove(pos);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };

                if is_tx_echo {
                    continue;
                }

                messages.push(CANMessage {
                    timestamp_us: base_timestamp + msg.timestamp as u64,
                    arb_id,
                    extended,
                    data,
                });
            }
        }

        Ok(messages)
    }

    pub fn clear_buffers(&self) -> Result<(), String> {
        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self.library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(self.channel_id, CLEAR_TX_BUFFER, std::ptr::null_mut(), std::ptr::null_mut());
            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_IOCTL_FAILED: CLEAR_TX_BUFFER error code {}", result));
            }

            let result = ioctl_fn(self.channel_id, CLEAR_RX_BUFFER, std::ptr::null_mut(), std::ptr::null_mut());
            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_IOCTL_FAILED: CLEAR_RX_BUFFER error code {}", result));
            }
        }

        Ok(())
    }

    /// Read version information from the device
    pub fn read_version(&self) -> Result<J2534VersionInfo, String> {
        let mut firmware_version = [0i8; 80];
        let mut dll_version = [0i8; 80];
        let mut api_version = [0i8; 80];

        unsafe {
            let read_version_fn: Symbol<PassThruReadVersionFn> = self.library
                .get(b"PassThruReadVersion\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruReadVersion - {}", e))?;

            let result = read_version_fn(
                self.device_id,
                firmware_version.as_mut_ptr(),
                dll_version.as_mut_ptr(),
                api_version.as_mut_ptr(),
            );

            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_READ_VERSION_FAILED: error code {} ({})", result, error_code_to_string(result)));
            }
        }

        fn cstr_to_string(arr: &[i8]) -> String {
            let bytes: Vec<u8> = arr.iter()
                .take_while(|&&b| b != 0)
                .map(|&b| b as u8)
                .collect();
            String::from_utf8_lossy(&bytes).to_string()
        }

        Ok(J2534VersionInfo {
            firmware_version: cstr_to_string(&firmware_version),
            dll_version: cstr_to_string(&dll_version),
            api_version: cstr_to_string(&api_version),
        })
    }

    /// Get the last error message from the DLL
    pub fn get_last_error(&self) -> Result<String, String> {
        let mut error_msg = [0i8; 80];

        unsafe {
            let get_last_error_fn: Symbol<PassThruGetLastErrorFn> = self.library
                .get(b"PassThruGetLastError\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruGetLastError - {}", e))?;

            get_last_error_fn(error_msg.as_mut_ptr());
        }

        let bytes: Vec<u8> = error_msg.iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as u8)
            .collect();
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    /// Read battery voltage (in millivolts, returned as volts)
    pub fn read_battery_voltage(&self) -> Result<f64, String> {
        let mut voltage: u32 = 0;

        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self.library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(
                self.channel_id,
                READ_VBATT,
                std::ptr::null_mut(),
                &mut voltage as *mut u32 as *mut c_void,
            );

            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_READ_VBATT_FAILED: error code {} ({})", result, error_code_to_string(result)));
            }
        }

        Ok(voltage as f64 / 1000.0)
    }

    /// Read programming voltage (in millivolts, returned as volts)
    pub fn read_programming_voltage(&self) -> Result<f64, String> {
        let mut voltage: u32 = 0;

        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self.library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(
                self.channel_id,
                READ_PROG_VOLTAGE,
                std::ptr::null_mut(),
                &mut voltage as *mut u32 as *mut c_void,
            );

            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_READ_PROG_VOLTAGE_FAILED: error code {} ({})", result, error_code_to_string(result)));
            }
        }

        Ok(voltage as f64 / 1000.0)
    }

    /// Start a periodic message
    pub fn start_periodic_message(&self, arb_id: u32, data: &[u8], interval_ms: u32, extended: bool) -> Result<u32, String> {
        let mut msg = PassThruMsg::default();
        msg.protocol_id = PROTOCOL_CAN;
        msg.tx_flags = if extended { CAN_29BIT_ID } else { 0 };

        // First 4 bytes are the CAN ID
        msg.data[0] = ((arb_id >> 24) & 0xFF) as u8;
        msg.data[1] = ((arb_id >> 16) & 0xFF) as u8;
        msg.data[2] = ((arb_id >> 8) & 0xFF) as u8;
        msg.data[3] = (arb_id & 0xFF) as u8;

        let data_len = data.len().min(8);
        msg.data[4..4 + data_len].copy_from_slice(&data[..data_len]);
        msg.data_size = (4 + data_len) as u32;

        let mut msg_id: c_ulong = 0;

        unsafe {
            let start_periodic_fn: Symbol<PassThruStartPeriodicMsgFn> = self.library
                .get(b"PassThruStartPeriodicMsg\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruStartPeriodicMsg - {}", e))?;

            let result = start_periodic_fn(self.channel_id, &msg, &mut msg_id, interval_ms);
            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_START_PERIODIC_FAILED: error code {} ({})", result, error_code_to_string(result)));
            }
        }

        Ok(msg_id)
    }

    /// Stop a periodic message
    pub fn stop_periodic_message(&self, msg_id: u32) -> Result<(), String> {
        unsafe {
            let stop_periodic_fn: Symbol<PassThruStopPeriodicMsgFn> = self.library
                .get(b"PassThruStopPeriodicMsg\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruStopPeriodicMsg - {}", e))?;

            let result = stop_periodic_fn(self.channel_id, msg_id);
            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_STOP_PERIODIC_FAILED: error code {} ({})", result, error_code_to_string(result)));
            }
        }

        Ok(())
    }

    /// Clear all periodic messages
    pub fn clear_periodic_messages(&self) -> Result<(), String> {
        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self.library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(self.channel_id, CLEAR_PERIODIC_MSGS, std::ptr::null_mut(), std::ptr::null_mut());
            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_CLEAR_PERIODIC_FAILED: error code {} ({})", result, error_code_to_string(result)));
            }
        }

        Ok(())
    }

    /// Add a message filter and return the filter ID
    pub fn add_filter(&self, filter_type: u32, mask: &[u8], pattern: &[u8], extended: bool) -> Result<u32, String> {
        let mut mask_msg = PassThruMsg::default();
        mask_msg.protocol_id = PROTOCOL_CAN;
        mask_msg.tx_flags = if extended { CAN_29BIT_ID } else { 0 };
        let mask_len = mask.len().min(4);
        mask_msg.data[..mask_len].copy_from_slice(&mask[..mask_len]);
        mask_msg.data_size = 4;

        let mut pattern_msg = PassThruMsg::default();
        pattern_msg.protocol_id = PROTOCOL_CAN;
        pattern_msg.tx_flags = if extended { CAN_29BIT_ID } else { 0 };
        let pattern_len = pattern.len().min(4);
        pattern_msg.data[..pattern_len].copy_from_slice(&pattern[..pattern_len]);
        pattern_msg.data_size = 4;

        let mut filter_id: c_ulong = 0;

        unsafe {
            let filter_fn: Symbol<PassThruStartMsgFilterFn> = self.library
                .get(b"PassThruStartMsgFilter\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruStartMsgFilter - {}", e))?;

            let result = filter_fn(self.channel_id, filter_type, &mask_msg, &pattern_msg, std::ptr::null(), &mut filter_id);
            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_ADD_FILTER_FAILED: error code {} ({})", result, error_code_to_string(result)));
            }
        }

        Ok(filter_id)
    }

    /// Remove a message filter
    pub fn remove_filter(&self, filter_id: u32) -> Result<(), String> {
        unsafe {
            let stop_filter_fn: Symbol<PassThruStopMsgFilterFn> = self.library
                .get(b"PassThruStopMsgFilter\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruStopMsgFilter - {}", e))?;

            let result = stop_filter_fn(self.channel_id, filter_id);
            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_REMOVE_FILTER_FAILED: error code {} ({})", result, error_code_to_string(result)));
            }
        }

        Ok(())
    }

    /// Clear all message filters
    pub fn clear_filters(&self) -> Result<(), String> {
        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self.library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(self.channel_id, CLEAR_MSG_FILTERS, std::ptr::null_mut(), std::ptr::null_mut());
            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_CLEAR_FILTERS_FAILED: error code {} ({})", result, error_code_to_string(result)));
            }
        }

        Ok(())
    }

    /// Get a configuration parameter value
    pub fn get_config(&self, parameter: u32) -> Result<u32, String> {
        let mut config = SConfig { parameter, value: 0 };
        let mut config_list = SConfigList {
            num_of_params: 1,
            config_ptr: &mut config,
        };

        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self.library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(
                self.channel_id,
                GET_CONFIG,
                &mut config_list as *mut SConfigList as *mut c_void,
                std::ptr::null_mut(),
            );

            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_GET_CONFIG_FAILED: error code {} ({})", result, error_code_to_string(result)));
            }
        }

        Ok(config.value)
    }

    /// Set a configuration parameter value
    pub fn set_config(&self, parameter: u32, value: u32) -> Result<(), String> {
        let mut config = SConfig { parameter, value };
        let mut config_list = SConfigList {
            num_of_params: 1,
            config_ptr: &mut config,
        };

        unsafe {
            let ioctl_fn: Symbol<PassThruIoctlFn> = self.library
                .get(b"PassThruIoctl\0")
                .map_err(|e| format!("ERR_J2534_FUNC_NOT_FOUND: PassThruIoctl - {}", e))?;

            let result = ioctl_fn(
                self.channel_id,
                SET_CONFIG,
                &mut config_list as *mut SConfigList as *mut c_void,
                std::ptr::null_mut(),
            );

            if result != STATUS_NOERROR {
                return Err(format!("ERR_J2534_SET_CONFIG_FAILED: error code {} ({})", result, error_code_to_string(result)));
            }
        }

        Ok(())
    }

    /// Get loopback setting
    pub fn get_loopback(&self) -> Result<bool, String> {
        self.get_config(LOOPBACK).map(|v| v != 0)
    }

    /// Set loopback setting
    pub fn set_loopback(&self, enabled: bool) -> Result<(), String> {
        self.set_config(LOOPBACK, if enabled { 1 } else { 0 })
    }

    /// Get the current data rate
    pub fn get_data_rate(&self) -> Result<u32, String> {
        self.get_config(DATA_RATE)
    }
}

impl Drop for J2534Connection {
    fn drop(&mut self) {
        unsafe {
            // Disconnect channel
            if let Ok(disconnect_fn) = self.library.get::<PassThruDisconnectFn>(b"PassThruDisconnect\0") {
                disconnect_fn(self.channel_id);
            }
            // Close device
            if let Ok(close_fn) = self.library.get::<PassThruCloseFn>(b"PassThruClose\0") {
                close_fn(self.device_id);
            }
        }
    }
}
