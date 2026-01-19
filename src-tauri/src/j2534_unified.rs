//! Unified J2534 Connection
//!
//! All J2534 DLLs are loaded out-of-process via the bridge for:
//! - Fault isolation (buggy DLL crashes don't kill the app)
//! - Cross-bitness support (64-bit app can use 32-bit DLLs and vice versa)
//! - Consistent behavior regardless of DLL architecture

use crate::bridge_client::{BridgeClient, Request, Response, ResponseData};
use crate::j2534::{self, CANMessage, J2534Progress, J2534VersionInfo};

/// Extended device info that includes bitness
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct J2534DeviceExt {
    pub name: String,
    pub vendor: String,
    pub dll_path: String,
    pub can_iso15765: bool,
    pub can_iso11898: bool,
    pub compatible: bool,
    pub bitness: u8,  // 32 or 64
    pub native: bool, // true if same bitness as app (still uses bridge for isolation)
}

/// Connection to a J2534 device via the bridge process
pub struct UnifiedConnection {
    bridge: BridgeClient,
}

impl UnifiedConnection {
    /// Enumerate all J2534 devices with bitness information
    pub fn enumerate_devices() -> Vec<J2534DeviceExt> {
        let process_bits = (std::mem::size_of::<usize>() * 8) as u8;

        j2534::enumerate_devices()
            .into_iter()
            .map(|d| {
                let dll_bits = get_dll_bitness(&d.dll_path).unwrap_or(process_bits);
                let is_native = dll_bits == process_bits;
                let clean_name = clean_device_name(&d.name);
                J2534DeviceExt {
                    name: clean_name,
                    vendor: d.vendor,
                    dll_path: d.dll_path,
                    can_iso15765: d.can_iso15765,
                    can_iso11898: d.can_iso11898,
                    compatible: true,
                    bitness: dll_bits,
                    native: is_native,
                }
            })
            .collect()
    }

    /// Open a connection to a device (always via bridge for fault isolation)
    pub fn open<F>(
        dll_path: &str,
        protocol_id: u32,
        baud_rate: u32,
        use_extended_id: bool,
        progress_callback: F,
    ) -> Result<Self, String>
    where
        F: Fn(J2534Progress) + Send + 'static,
    {
        let process_bits = (std::mem::size_of::<usize>() * 8) as u8;
        let dll_bits = get_dll_bitness(dll_path).unwrap_or(process_bits);

        progress_callback(J2534Progress {
            step: "load_dll".to_string(),
            status: "in_progress".to_string(),
            message: Some(format!("Starting {}-bit bridge process", dll_bits)),
        });

        let mut bridge = BridgeClient::new();
        bridge.start(dll_bits)?;

        progress_callback(J2534Progress {
            step: "load_dll".to_string(),
            status: "in_progress".to_string(),
            message: Some("Bridge started, opening device".to_string()),
        });

        let response = bridge.send_request(Request::Open {
            dll_path: dll_path.to_string(),
            protocol_id,
            baud_rate,
            use_extended_id,
        })?;

        match response {
            Response::Ok { .. } => {
                progress_callback(J2534Progress {
                    step: "complete".to_string(),
                    status: "success".to_string(),
                    message: Some("Connected via bridge".to_string()),
                });
                Ok(UnifiedConnection { bridge })
            }
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Send a CAN message
    pub fn send_message(&mut self, arb_id: u32, data: &[u8], extended: bool) -> Result<(), String> {
        let response = self.bridge.send_request(Request::SendMessage {
            arb_id,
            data: data.to_vec(),
            extended,
        })?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Read CAN messages
    pub fn read_messages(&mut self, timeout_ms: u32) -> Result<Vec<CANMessage>, String> {
        let response = self.bridge.send_request(Request::ReadMessages { timeout_ms })?;
        match response {
            Response::Ok { data: ResponseData::Messages(msgs) } => {
                Ok(msgs.into_iter().map(|m| CANMessage {
                    timestamp_us: m.timestamp_us,
                    arb_id: m.arb_id,
                    extended: m.extended,
                    data: m.data,
                }).collect())
            }
            Response::Ok { .. } => Ok(Vec::new()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Read CAN messages including loopback echoes (for sanity testing)
    pub fn read_messages_with_loopback(&mut self, timeout_ms: u32) -> Result<Vec<CANMessage>, String> {
        let response = self.bridge.send_request(Request::ReadMessagesWithLoopback { timeout_ms })?;
        match response {
            Response::Ok { data: ResponseData::Messages(msgs) } => {
                Ok(msgs.into_iter().map(|m| CANMessage {
                    timestamp_us: m.timestamp_us,
                    arb_id: m.arb_id,
                    extended: m.extended,
                    data: m.data,
                }).collect())
            }
            Response::Ok { .. } => Ok(Vec::new()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Clear TX and RX buffers
    pub fn clear_buffers(&mut self) -> Result<(), String> {
        let response = self.bridge.send_request(Request::ClearBuffers)?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Read version information
    pub fn read_version(&mut self) -> Result<J2534VersionInfo, String> {
        let response = self.bridge.send_request(Request::ReadVersion)?;
        match response {
            Response::Ok { data: ResponseData::Version(v) } => {
                Ok(J2534VersionInfo {
                    firmware_version: v.firmware_version,
                    dll_version: v.dll_version,
                    api_version: v.api_version,
                })
            }
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Get last error string
    pub fn get_last_error(&mut self) -> Result<String, String> {
        let response = self.bridge.send_request(Request::GetLastError)?;
        match response {
            Response::Ok { data: ResponseData::String(s) } => Ok(s),
            Response::Ok { .. } => Ok(String::new()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Read battery voltage
    pub fn read_battery_voltage(&mut self) -> Result<f64, String> {
        let response = self.bridge.send_request(Request::ReadBatteryVoltage)?;
        match response {
            Response::Ok { data: ResponseData::Float(v) } => Ok(v),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Read programming voltage
    pub fn read_programming_voltage(&mut self) -> Result<f64, String> {
        let response = self.bridge.send_request(Request::ReadProgrammingVoltage)?;
        match response {
            Response::Ok { data: ResponseData::Float(v) } => Ok(v),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Start a periodic message
    pub fn start_periodic_message(
        &mut self,
        arb_id: u32,
        data: &[u8],
        interval_ms: u32,
        extended: bool,
    ) -> Result<u32, String> {
        let response = self.bridge.send_request(Request::StartPeriodicMessage {
            arb_id,
            data: data.to_vec(),
            interval_ms,
            extended,
        })?;
        match response {
            Response::Ok { data: ResponseData::Number(id) } => Ok(id),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Stop a periodic message
    pub fn stop_periodic_message(&mut self, msg_id: u32) -> Result<(), String> {
        let response = self.bridge.send_request(Request::StopPeriodicMessage { msg_id })?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Clear all periodic messages
    pub fn clear_periodic_messages(&mut self) -> Result<(), String> {
        let response = self.bridge.send_request(Request::ClearPeriodicMessages)?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Add a message filter
    pub fn add_filter(
        &mut self,
        filter_type: u32,
        mask: &[u8],
        pattern: &[u8],
        extended: bool,
    ) -> Result<u32, String> {
        let ft = match filter_type {
            1 => "pass",
            2 => "block",
            3 => "flow_control",
            _ => return Err("Invalid filter type".to_string()),
        };
        let response = self.bridge.send_request(Request::AddFilter {
            filter_type: ft.to_string(),
            mask: mask.to_vec(),
            pattern: pattern.to_vec(),
            extended,
        })?;
        match response {
            Response::Ok { data: ResponseData::Number(id) } => Ok(id),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Remove a message filter
    pub fn remove_filter(&mut self, filter_id: u32) -> Result<(), String> {
        let response = self.bridge.send_request(Request::RemoveFilter { filter_id })?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Clear all filters
    pub fn clear_filters(&mut self) -> Result<(), String> {
        let response = self.bridge.send_request(Request::ClearFilters)?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Get a configuration parameter
    pub fn get_config(&mut self, parameter: u32) -> Result<u32, String> {
        let response = self.bridge.send_request(Request::GetConfig { parameter })?;
        match response {
            Response::Ok { data: ResponseData::Number(v) } => Ok(v),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Set a configuration parameter
    pub fn set_config(&mut self, parameter: u32, value: u32) -> Result<(), String> {
        let response = self.bridge.send_request(Request::SetConfig { parameter, value })?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Get loopback setting
    pub fn get_loopback(&mut self) -> Result<bool, String> {
        let response = self.bridge.send_request(Request::GetLoopback)?;
        match response {
            Response::Ok { data: ResponseData::Bool(v) } => Ok(v),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Set loopback setting
    pub fn set_loopback(&mut self, enabled: bool) -> Result<(), String> {
        let response = self.bridge.send_request(Request::SetLoopback { enabled })?;
        match response {
            Response::Ok { .. } => Ok(()),
            Response::Error { message, .. } => Err(message),
        }
    }

    /// Get current data rate
    pub fn get_data_rate(&mut self) -> Result<u32, String> {
        let response = self.bridge.send_request(Request::GetDataRate)?;
        match response {
            Response::Ok { data: ResponseData::Number(v) } => Ok(v),
            Response::Ok { .. } => Err("Unexpected response type".to_string()),
            Response::Error { message, .. } => Err(message),
        }
    }
}

impl Drop for UnifiedConnection {
    fn drop(&mut self) {
        let _ = self.bridge.stop();
    }
}

/// Clean up device name by removing legacy bitness/compatibility markers
fn clean_device_name(name: &str) -> String {
    let mut clean = name.to_string();
    let markers = [
        "(64-bit)",
        "(32-bit)",
        "(64 bit)",
        "(32 bit)",
        "(incompatible)",
        "(compatible)",
        "(x64)",
        "(x86)",
    ];
    for marker in markers {
        clean = clean.replace(marker, "");
    }
    clean.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Determine the bitness of a DLL by reading its PE header
fn get_dll_bitness(dll_path: &str) -> Option<u8> {
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open(dll_path).ok()?;
    let mut header = [0u8; 64];
    file.read_exact(&mut header).ok()?;

    if header[0] != 0x4D || header[1] != 0x5A {
        return None;
    }

    let pe_offset = u32::from_le_bytes([header[0x3C], header[0x3D], header[0x3E], header[0x3F]]) as usize;

    let mut pe_header = [0u8; 6];
    use std::io::{Seek, SeekFrom};
    let mut file = File::open(dll_path).ok()?;
    file.seek(SeekFrom::Start(pe_offset as u64)).ok()?;
    file.read_exact(&mut pe_header).ok()?;

    if pe_header[0] != 0x50 || pe_header[1] != 0x45 || pe_header[2] != 0x00 || pe_header[3] != 0x00 {
        return None;
    }

    let machine = u16::from_le_bytes([pe_header[4], pe_header[5]]);

    match machine {
        0x014c => Some(32), // IMAGE_FILE_MACHINE_I386
        0x8664 => Some(64), // IMAGE_FILE_MACHINE_AMD64
        0xAA64 => Some(64), // IMAGE_FILE_MACHINE_ARM64
        _ => None,
    }
}
