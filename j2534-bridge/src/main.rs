//! J2534 Bridge Process
//!
//! A standalone process that loads J2534 DLLs and communicates with the main app
//! via named pipes. This allows a 64-bit app to use 32-bit DLLs (via a 32-bit bridge)
//! and vice versa.
//!
//! Usage: j2534-bridge.exe <pipe_name>

mod j2534;
mod protocol;

use protocol::{CanMessage, DeviceInfo, Message, Request, Response, ResponseData, VersionInfo};
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};

#[cfg(windows)]
use std::os::windows::io::FromRawHandle;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: j2534-bridge.exe <pipe_name>");
        eprintln!("Example: j2534-bridge.exe \\\\.\\pipe\\jester-j2534-1234");
        std::process::exit(1);
    }

    let pipe_name = &args[1];
    eprintln!("[bridge] Starting J2534 bridge on pipe: {}", pipe_name);
    eprintln!(
        "[bridge] Process bitness: {}-bit",
        std::mem::size_of::<usize>() * 8
    );

    if let Err(e) = run_bridge(pipe_name) {
        eprintln!("[bridge] Fatal error: {}", e);
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run_bridge(pipe_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    use windows::core::PCSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeA, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
    };

    // Create the named pipe
    let pipe_name_cstr = std::ffi::CString::new(pipe_name)?;
    let pipe_handle = unsafe {
        CreateNamedPipeA(
            PCSTR::from_raw(pipe_name_cstr.as_ptr() as *const u8),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1,    // max instances
            4096, // out buffer size
            4096, // in buffer size
            0,    // default timeout
            None, // default security
        )
    }?;

    eprintln!("[bridge] Waiting for client connection...");

    // Wait for client to connect
    let connect_result = unsafe { ConnectNamedPipe(pipe_handle, None) };
    if connect_result.is_err() {
        let err = std::io::Error::last_os_error();
        // ERROR_PIPE_CONNECTED (535) means client connected before we called ConnectNamedPipe
        if err.raw_os_error() != Some(535) {
            let _ = unsafe { CloseHandle(pipe_handle) };
            return Err(format!("Failed to connect pipe: {}", err).into());
        }
    }

    eprintln!("[bridge] Client connected");

    // Convert to std File for easier I/O
    let handle_raw = pipe_handle.0 as *mut std::ffi::c_void;
    let file = unsafe { std::fs::File::from_raw_handle(handle_raw) };

    // Run the message loop
    let result = message_loop(file);

    // Note: file is dropped here, which closes the handle

    result
}

#[cfg(not(windows))]
fn run_bridge(_pipe_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    Err("J2534 bridge is only supported on Windows".into())
}

fn message_loop(pipe: std::fs::File) -> Result<(), Box<dyn std::error::Error>> {
    let reader = BufReader::new(pipe.try_clone()?);
    let mut writer = pipe;

    let connection: Arc<Mutex<Option<j2534::J2534Connection>>> = Arc::new(Mutex::new(None));

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[bridge] Read error: {}", e);
                break;
            }
        };

        if line.is_empty() {
            continue;
        }

        // Parse the request
        let msg: Message<Request> = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[bridge] Parse error: {}", e);
                let response = Message {
                    id: 0,
                    payload: Response::error(-1, format!("Parse error: {}", e)),
                };
                send_response(&mut writer, &response)?;
                continue;
            }
        };

        eprintln!("[bridge] Request {}: {:?}", msg.id, msg.payload);

        // Handle the request
        let response_payload = handle_request(&msg.payload, &connection);

        let response = Message {
            id: msg.id,
            payload: response_payload,
        };

        send_response(&mut writer, &response)?;

        // Check for shutdown
        if matches!(msg.payload, Request::Shutdown) {
            eprintln!("[bridge] Shutdown requested");
            break;
        }
    }

    eprintln!("[bridge] Message loop ended");
    Ok(())
}

fn send_response(
    writer: &mut std::fs::File,
    response: &Message<Response>,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(response)?;
    writeln!(writer, "{}", json)?;
    writer.flush()?;
    Ok(())
}

fn handle_request(
    request: &Request,
    connection: &Arc<Mutex<Option<j2534::J2534Connection>>>,
) -> Response {
    match request {
        Request::EnumerateDevices => {
            let devices = j2534::enumerate_devices();
            let device_infos: Vec<DeviceInfo> = devices
                .into_iter()
                .map(|d| DeviceInfo {
                    name: d.name,
                    vendor: d.vendor,
                    dll_path: d.dll_path,
                    can_iso15765: d.can_iso15765,
                    can_iso11898: d.can_iso11898,
                    compatible: d.compatible,
                    bitness: if d.compatible {
                        (std::mem::size_of::<usize>() * 8) as u8
                    } else {
                        if (std::mem::size_of::<usize>() * 8) == 64 {
                            32
                        } else {
                            64
                        }
                    },
                })
                .collect();
            Response::ok(ResponseData::Devices(device_infos))
        }

        Request::Open {
            dll_path,
            protocol_id,
            baud_rate,
            use_extended_id,
        } => {
            let mut conn_guard = connection.lock().unwrap();
            if conn_guard.is_some() {
                return Response::error(-1, "Already connected");
            }

            match j2534::J2534Connection::open(dll_path, *protocol_id, *baud_rate, *use_extended_id, |_| {}) {
                Ok(conn) => {
                    *conn_guard = Some(conn);
                    Response::ok(ResponseData::Connected)
                }
                Err(e) => Response::error(-1, e),
            }
        }

        Request::Close => {
            let mut conn_guard = connection.lock().unwrap();
            *conn_guard = None;
            Response::ok_none()
        }

        Request::SendMessage { arb_id, data, extended } => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.send_message(*arb_id, data, *extended) {
                    Ok(()) => Response::ok_none(),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::ReadMessages { timeout_ms } => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.read_messages(*timeout_ms) {
                    Ok(messages) => {
                        let can_messages: Vec<CanMessage> = messages
                            .into_iter()
                            .map(|m| CanMessage {
                                timestamp_us: m.timestamp_us,
                                arb_id: m.arb_id,
                                extended: m.extended,
                                data: m.data,
                            })
                            .collect();
                        Response::ok(ResponseData::Messages(can_messages))
                    }
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::ClearBuffers => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.clear_buffers() {
                    Ok(()) => Response::ok_none(),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::ReadVersion => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.read_version() {
                    Ok(v) => Response::ok(ResponseData::Version(VersionInfo {
                        firmware_version: v.firmware_version,
                        dll_version: v.dll_version,
                        api_version: v.api_version,
                    })),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::GetLastError => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.get_last_error() {
                    Ok(s) => Response::ok(ResponseData::String(s)),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::ReadBatteryVoltage => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.read_battery_voltage() {
                    Ok(v) => Response::ok(ResponseData::Float(v)),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::ReadProgrammingVoltage => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.read_programming_voltage() {
                    Ok(v) => Response::ok(ResponseData::Float(v)),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::StartPeriodicMessage {
            arb_id,
            data,
            interval_ms,
            extended,
        } => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.start_periodic_message(*arb_id, data, *interval_ms, *extended) {
                    Ok(id) => Response::ok(ResponseData::Number(id)),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::StopPeriodicMessage { msg_id } => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.stop_periodic_message(*msg_id) {
                    Ok(()) => Response::ok_none(),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::ClearPeriodicMessages => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.clear_periodic_messages() {
                    Ok(()) => Response::ok_none(),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::AddFilter {
            filter_type,
            mask,
            pattern,
            extended,
        } => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => {
                    let ft = match filter_type.as_str() {
                        "pass" => j2534::PASS_FILTER,
                        "block" => j2534::BLOCK_FILTER,
                        "flow_control" => j2534::FLOW_CONTROL_FILTER,
                        _ => return Response::error(-1, "Invalid filter type"),
                    };
                    match conn.add_filter(ft, mask, pattern, *extended) {
                        Ok(id) => Response::ok(ResponseData::Number(id)),
                        Err(e) => Response::error(-1, e),
                    }
                }
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::RemoveFilter { filter_id } => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.remove_filter(*filter_id) {
                    Ok(()) => Response::ok_none(),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::ClearFilters => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.clear_filters() {
                    Ok(()) => Response::ok_none(),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::GetConfig { parameter } => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.get_config(*parameter) {
                    Ok(v) => Response::ok(ResponseData::Number(v)),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::SetConfig { parameter, value } => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.set_config(*parameter, *value) {
                    Ok(()) => Response::ok_none(),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::GetLoopback => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.get_loopback() {
                    Ok(v) => Response::ok(ResponseData::Bool(v)),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::SetLoopback { enabled } => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.set_loopback(*enabled) {
                    Ok(()) => Response::ok_none(),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::GetDataRate => {
            let conn_guard = connection.lock().unwrap();
            match conn_guard.as_ref() {
                Some(conn) => match conn.get_data_rate() {
                    Ok(v) => Response::ok(ResponseData::Number(v)),
                    Err(e) => Response::error(-1, e),
                },
                None => Response::error(-1, "Not connected"),
            }
        }

        Request::Shutdown => Response::ok_none(),
    }
}
