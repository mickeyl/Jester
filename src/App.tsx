import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  J2534Device,
  J2534Config,
  CANMessage,
  ConnectionStatus,
  J2534Progress,
  PlatformInfo,
  SendMessageRequest,
  J2534VersionInfo,
  PeriodicMessageRequest,
  FilterRequest,
  TestResult,
  ActivePeriodicMessage,
  ActiveFilter,
} from "./types";
import { J2534ConfigParams, J2534ConfigParamNames } from "./types";

interface LogEntry {
  id: number;
  timestamp: Date;
  direction: "tx" | "rx" | "error" | "info";
  arbId?: number;
  extended?: boolean;
  data?: number[];
  message?: string;
}

type TabType = "messages" | "device-info" | "api-test";

const STEP_LABELS: Record<string, string> = {
  load_dll: "Load J2534 DLL",
  open_device: "Open Device",
  connect_channel: "Connect CAN Channel",
  set_filter: "Configure Message Filter",
  loopback: "Loopback Settings",
  complete: "Connection Complete",
};

function formatHex(value: number, digits: number): string {
  return value.toString(16).toUpperCase().padStart(digits, "0");
}

function formatData(data: number[]): string {
  return data.map((b) => formatHex(b, 2)).join(" ");
}

function formatTimestamp(date: Date): string {
  const hours = date.getHours().toString().padStart(2, "0");
  const minutes = date.getMinutes().toString().padStart(2, "0");
  const seconds = date.getSeconds().toString().padStart(2, "0");
  const ms = date.getMilliseconds().toString().padStart(3, "0");
  return `${hours}:${minutes}:${seconds}.${ms}`;
}

function parseHexInput(input: string): number[] {
  const cleaned = input.replace(/[^0-9a-fA-F\s]/g, "");
  const bytes = cleaned.split(/\s+/).filter((s) => s.length > 0);
  return bytes.map((b) => parseInt(b, 16)).filter((n) => !isNaN(n));
}

function App() {
  // Platform info
  const [platformInfo, setPlatformInfo] = useState<PlatformInfo | null>(null);

  // Device list
  const [devices, setDevices] = useState<J2534Device[]>([]);
  const [selectedDevice, setSelectedDevice] = useState("");

  // Connection config
  const [baudRate, setBaudRate] = useState(500000);
  const [useExtendedId, setUseExtendedId] = useState(false);

  // Connection state
  const [status, setStatus] = useState<ConnectionStatus>({
    connected: false,
    deviceName: "",
    baudRate: 0,
    messagesSent: 0,
    messagesReceived: 0,
  });

  // Progress dialog
  const [showProgress, setShowProgress] = useState(false);
  const [progressSteps, setProgressSteps] = useState<J2534Progress[]>([]);

  // Message sending - load initial values from localStorage
  const [txArbId, setTxArbId] = useState(() => localStorage.getItem("jester.txArbId") || "7E0");
  const [txData, setTxData] = useState(() => localStorage.getItem("jester.txData") || "01 00");
  const [txExtended, setTxExtended] = useState(() => localStorage.getItem("jester.txExtended") === "true");

  // Message log
  const [logEntries, setLogEntries] = useState<LogEntry[]>([]);
  const [autoScroll, setAutoScroll] = useState(true);
  const logContainerRef = useRef<HTMLDivElement>(null);
  const logIdRef = useRef(0);

  // Polling state
  const pollingRef = useRef<number | null>(null);

  // Error state
  const [error, setError] = useState<string | null>(null);

  // Active tab
  const [activeTab, setActiveTab] = useState<TabType>("messages");

  // Device info
  const [versionInfo, setVersionInfo] = useState<J2534VersionInfo | null>(null);
  const [batteryVoltage, setBatteryVoltage] = useState<number | null>(null);
  const [progVoltage, setProgVoltage] = useState<number | null>(null);
  const [loopbackEnabled, setLoopbackEnabled] = useState<boolean | null>(null);
  const [currentDataRate, setCurrentDataRate] = useState<number | null>(null);

  // API Testing state
  const [testResults, setTestResults] = useState<TestResult[]>([]);
  const [activePeriodicMsgs, setActivePeriodicMsgs] = useState<ActivePeriodicMessage[]>([]);
  const [activeFilters, setActiveFilters] = useState<ActiveFilter[]>([]);

  // Periodic message form
  const [periodicArbId, setPeriodicArbId] = useState("7DF");
  const [periodicData, setPeriodicData] = useState("02 01 00");
  const [periodicInterval, setPeriodicInterval] = useState(100);
  const [periodicExtended, setPeriodicExtended] = useState(false);

  // Filter form
  const [filterType, setFilterType] = useState<"pass" | "block" | "flow_control">("pass");
  const [filterMask, setFilterMask] = useState("00 00 00 00");
  const [filterPattern, setFilterPattern] = useState("00 00 00 00");
  const [filterExtended, setFilterExtended] = useState(false);

  // Config form
  const [configParam, setConfigParam] = useState<number>(J2534ConfigParams.DATA_RATE);
  const [configValue, setConfigValue] = useState("");
  const [configReadValue, setConfigReadValue] = useState<number | null>(null);

  // About dialog - show on first launch
  const [showAbout, setShowAbout] = useState(true);

  // Load platform info and devices on mount
  useEffect(() => {
    invoke<PlatformInfo>("get_platform_info").then(setPlatformInfo);
    refreshDevices();
  }, []);

  // Listen for progress events
  useEffect(() => {
    const unlisten = listen<J2534Progress>("j2534-progress", (event) => {
      setProgressSteps((prev) => {
        const existing = prev.find((s) => s.step === event.payload.step);
        if (existing) {
          return prev.map((s) =>
            s.step === event.payload.step ? event.payload : s
          );
        }
        return [...prev, event.payload];
      });

      // Close progress dialog on completion or error
      if (
        event.payload.status === "success" &&
        event.payload.step === "complete"
      ) {
        setTimeout(() => setShowProgress(false), 500);
      } else if (event.payload.status === "error") {
        setTimeout(() => setShowProgress(false), 2000);
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Auto-scroll log
  useEffect(() => {
    if (autoScroll && logContainerRef.current) {
      logContainerRef.current.scrollTop = logContainerRef.current.scrollHeight;
    }
  }, [logEntries, autoScroll]);

  // Persist send message fields to localStorage
  useEffect(() => {
    localStorage.setItem("jester.txArbId", txArbId);
  }, [txArbId]);

  useEffect(() => {
    localStorage.setItem("jester.txData", txData);
  }, [txData]);

  useEffect(() => {
    localStorage.setItem("jester.txExtended", String(txExtended));
  }, [txExtended]);

  // Polling for messages when connected
  useEffect(() => {
    if (status.connected) {
      const poll = async () => {
        try {
          const messages = await invoke<CANMessage[]>("j2534_read_messages", {
            timeoutMs: 100,
          });

          if (messages.length > 0) {
            setLogEntries((prev) => [
              ...prev,
              ...messages.map((msg) => ({
                id: ++logIdRef.current,
                timestamp: new Date(msg.timestampUs / 1000),
                direction: "rx" as const,
                arbId: msg.arbId,
                extended: msg.extended,
                data: msg.data,
              })),
            ]);

            // Update status
            const newStatus = await invoke<ConnectionStatus>(
              "j2534_get_status"
            );
            setStatus(newStatus);
          }
        } catch (err) {
          // Ignore buffer empty errors
          const errStr = String(err);
          if (!errStr.includes("ERR_BUFFER_EMPTY")) {
            console.error("Read error:", err);
          }
        }

        pollingRef.current = window.setTimeout(poll, 50);
      };

      poll();

      return () => {
        if (pollingRef.current) {
          clearTimeout(pollingRef.current);
          pollingRef.current = null;
        }
      };
    }
  }, [status.connected]);

  // Fetch device info when connected
  useEffect(() => {
    if (status.connected) {
      fetchDeviceInfo();
    } else {
      // Clear device info when disconnected
      setVersionInfo(null);
      setBatteryVoltage(null);
      setProgVoltage(null);
      setLoopbackEnabled(null);
      setCurrentDataRate(null);
      setActivePeriodicMsgs([]);
      setActiveFilters([]);
    }
  }, [status.connected]);

  const fetchDeviceInfo = async () => {
    try {
      const version = await invoke<J2534VersionInfo>("j2534_read_version");
      setVersionInfo(version);
    } catch (err) {
      console.error("Failed to read version:", err);
    }

    try {
      const voltage = await invoke<number>("j2534_read_battery_voltage");
      setBatteryVoltage(voltage);
    } catch (err) {
      console.error("Failed to read battery voltage:", err);
    }

    try {
      const voltage = await invoke<number>("j2534_read_programming_voltage");
      setProgVoltage(voltage);
    } catch (err) {
      console.error("Failed to read programming voltage:", err);
    }

    try {
      const loopback = await invoke<boolean>("j2534_get_loopback");
      setLoopbackEnabled(loopback);
    } catch (err) {
      console.error("Failed to get loopback:", err);
    }

    try {
      const dataRate = await invoke<number>("j2534_get_data_rate");
      setCurrentDataRate(dataRate);
    } catch (err) {
      console.error("Failed to get data rate:", err);
    }
  };

  const refreshDevices = useCallback(async () => {
    try {
      const deviceList = await invoke<J2534Device[]>("j2534_enumerate_devices");
      setDevices(deviceList);
      if (deviceList.length > 0 && !selectedDevice) {
        const compatible = deviceList.find((d) => d.compatible);
        if (compatible) {
          setSelectedDevice(compatible.name);
        }
      }
    } catch (err) {
      console.error("Failed to enumerate devices:", err);
    }
  }, [selectedDevice]);

  const handleConnect = useCallback(async () => {
    if (!selectedDevice) return;

    setError(null);
    setProgressSteps([]);
    setShowProgress(true);

    try {
      const config: J2534Config = {
        deviceName: selectedDevice,
        baudRate,
        protocol: "CAN",
        useExtendedId,
      };

      await invoke("j2534_connect", { config });

      const newStatus = await invoke<ConnectionStatus>("j2534_get_status");
      setStatus(newStatus);
    } catch (err) {
      setError(String(err));
      setShowProgress(false);
    }
  }, [selectedDevice, baudRate, useExtendedId]);

  const handleDisconnect = useCallback(async () => {
    try {
      await invoke("j2534_disconnect");
      setStatus((prev) => ({ ...prev, connected: false }));
    } catch (err) {
      setError(String(err));
    }
  }, []);

  const handleSendMessage = useCallback(async () => {
    if (!status.connected) return;

    const arbId = parseInt(txArbId, 16);
    if (isNaN(arbId)) {
      setError("Invalid arbitration ID");
      return;
    }

    const data = parseHexInput(txData);
    if (data.length === 0 || data.length > 8) {
      setError("Data must be 1-8 bytes");
      return;
    }

    setError(null);

    try {
      const request: SendMessageRequest = {
        arbId,
        data,
        extended: txExtended,
      };

      await invoke("j2534_send_message", { request });

      // Add to log
      setLogEntries((prev) => [
        ...prev,
        {
          id: ++logIdRef.current,
          timestamp: new Date(),
          direction: "tx",
          arbId,
          extended: txExtended,
          data,
        },
      ]);

      // Update status
      const newStatus = await invoke<ConnectionStatus>("j2534_get_status");
      setStatus(newStatus);
    } catch (err) {
      setError(String(err));
      setLogEntries((prev) => [
        ...prev,
        {
          id: ++logIdRef.current,
          timestamp: new Date(),
          direction: "error",
          message: String(err),
        },
      ]);
    }
  }, [status.connected, txArbId, txData, txExtended]);

  const handleClearLog = useCallback(() => {
    setLogEntries([]);
  }, []);

  const handleClearBuffers = useCallback(async () => {
    if (!status.connected) return;

    try {
      await invoke("j2534_clear_buffers");
      addTestResult("Clear Buffers", true, "Buffers cleared successfully");
    } catch (err) {
      addTestResult("Clear Buffers", false, String(err));
    }
  }, [status.connected]);

  const addTestResult = (name: string, success: boolean, message: string) => {
    setTestResults((prev) => [
      { name, success, message, timestamp: Date.now() },
      ...prev.slice(0, 49), // Keep last 50 results
    ]);
  };

  // API Test functions
  const handleGetLastError = async () => {
    try {
      const error = await invoke<string>("j2534_get_last_error");
      addTestResult("GetLastError", true, error || "(no error)");
    } catch (err) {
      addTestResult("GetLastError", false, String(err));
    }
  };

  const handleStartPeriodicMessage = async () => {
    const arbId = parseInt(periodicArbId, 16);
    if (isNaN(arbId)) {
      addTestResult("Start Periodic", false, "Invalid arbitration ID");
      return;
    }

    const data = parseHexInput(periodicData);
    if (data.length === 0 || data.length > 8) {
      addTestResult("Start Periodic", false, "Data must be 1-8 bytes");
      return;
    }

    try {
      const request: PeriodicMessageRequest = {
        arbId,
        data,
        intervalMs: periodicInterval,
        extended: periodicExtended,
      };

      const msgId = await invoke<number>("j2534_start_periodic_message", { request });
      addTestResult("Start Periodic", true, `Message ID: ${msgId}`);

      setActivePeriodicMsgs((prev) => [
        ...prev,
        { msgId, arbId, data, intervalMs: periodicInterval },
      ]);
    } catch (err) {
      addTestResult("Start Periodic", false, String(err));
    }
  };

  const handleStopPeriodicMessage = async (msgId: number) => {
    try {
      await invoke("j2534_stop_periodic_message", { msgId });
      addTestResult("Stop Periodic", true, `Stopped message ID: ${msgId}`);
      setActivePeriodicMsgs((prev) => prev.filter((m) => m.msgId !== msgId));
    } catch (err) {
      addTestResult("Stop Periodic", false, String(err));
    }
  };

  const handleClearPeriodicMessages = async () => {
    try {
      await invoke("j2534_clear_periodic_messages");
      addTestResult("Clear Periodic", true, "All periodic messages cleared");
      setActivePeriodicMsgs([]);
    } catch (err) {
      addTestResult("Clear Periodic", false, String(err));
    }
  };

  const handleAddFilter = async () => {
    const mask = parseHexInput(filterMask);
    const pattern = parseHexInput(filterPattern);

    if (mask.length !== 4) {
      addTestResult("Add Filter", false, "Mask must be 4 bytes");
      return;
    }
    if (pattern.length !== 4) {
      addTestResult("Add Filter", false, "Pattern must be 4 bytes");
      return;
    }

    try {
      const request: FilterRequest = {
        filterType,
        mask,
        pattern,
        extended: filterExtended,
      };

      const filterId = await invoke<number>("j2534_add_filter", { request });
      addTestResult("Add Filter", true, `Filter ID: ${filterId}`);

      setActiveFilters((prev) => [
        ...prev,
        { filterId, filterType, mask, pattern },
      ]);
    } catch (err) {
      addTestResult("Add Filter", false, String(err));
    }
  };

  const handleRemoveFilter = async (filterId: number) => {
    try {
      await invoke("j2534_remove_filter", { filterId });
      addTestResult("Remove Filter", true, `Removed filter ID: ${filterId}`);
      setActiveFilters((prev) => prev.filter((f) => f.filterId !== filterId));
    } catch (err) {
      addTestResult("Remove Filter", false, String(err));
    }
  };

  const handleClearFilters = async () => {
    try {
      await invoke("j2534_clear_filters");
      addTestResult("Clear Filters", true, "All filters cleared");
      setActiveFilters([]);
    } catch (err) {
      addTestResult("Clear Filters", false, String(err));
    }
  };

  const handleGetConfig = async () => {
    try {
      const value = await invoke<number>("j2534_get_config", { parameter: configParam });
      setConfigReadValue(value);
      const paramName = J2534ConfigParamNames[configParam] || `0x${configParam.toString(16)}`;
      addTestResult("Get Config", true, `${paramName} = ${value}`);
    } catch (err) {
      addTestResult("Get Config", false, String(err));
    }
  };

  const handleSetConfig = async () => {
    const value = parseInt(configValue);
    if (isNaN(value)) {
      addTestResult("Set Config", false, "Invalid value");
      return;
    }

    try {
      await invoke("j2534_set_config", { parameter: configParam, value });
      const paramName = J2534ConfigParamNames[configParam] || `0x${configParam.toString(16)}`;
      addTestResult("Set Config", true, `${paramName} set to ${value}`);
    } catch (err) {
      addTestResult("Set Config", false, String(err));
    }
  };

  const handleToggleLoopback = async () => {
    if (loopbackEnabled === null) return;

    try {
      await invoke("j2534_set_loopback", { enabled: !loopbackEnabled });
      setLoopbackEnabled(!loopbackEnabled);
      addTestResult("Set Loopback", true, `Loopback ${!loopbackEnabled ? "enabled" : "disabled"}`);
    } catch (err) {
      addTestResult("Set Loopback", false, String(err));
    }
  };

  const handleReadVersion = async () => {
    try {
      const version = await invoke<J2534VersionInfo>("j2534_read_version");
      setVersionInfo(version);
      addTestResult(
        "Read Version",
        true,
        `FW: ${version.firmwareVersion}, DLL: ${version.dllVersion}, API: ${version.apiVersion}`
      );
    } catch (err) {
      addTestResult("Read Version", false, String(err));
    }
  };

  const isWindows = platformInfo?.platform === "windows";

  return (
    <div className="app-container">
      {/* Connection Panel */}
      <div className="connection-panel">
        <div className="panel-header">
          <h2>J2534 Connection</h2>
          <button className="about-btn" onClick={() => setShowAbout(true)} title="About Jester">?</button>
        </div>

        {error && <div className="error-message">{error}</div>}

        {!isWindows && platformInfo && (
          <div className="error-message">
            J2534 is only supported on Windows. Current platform:{" "}
            {platformInfo.platform}
          </div>
        )}

        <div className="connection-row">
          <div className="form-group">
            <label>Device</label>
            <select
              value={selectedDevice}
              onChange={(e) => setSelectedDevice(e.target.value)}
              disabled={status.connected || !isWindows}
            >
              <option value="">Select a device...</option>
              {devices.map((device) => (
                <option
                  key={device.dllPath}
                  value={device.name}
                  disabled={!device.compatible}
                >
                  {device.name} ({device.vendor})
                </option>
              ))}
            </select>
          </div>

          <div className="form-group">
            <label>Baud Rate</label>
            <select
              value={baudRate}
              onChange={(e) => setBaudRate(Number(e.target.value))}
              disabled={status.connected}
            >
              <option value={125000}>125 kbps</option>
              <option value={250000}>250 kbps</option>
              <option value={500000}>500 kbps</option>
              <option value={1000000}>1 Mbps</option>
            </select>
          </div>

          <div className="checkbox-group">
            <input
              type="checkbox"
              id="extendedId"
              checked={useExtendedId}
              onChange={(e) => setUseExtendedId(e.target.checked)}
              disabled={status.connected}
            />
            <label htmlFor="extendedId">Extended IDs (29-bit)</label>
          </div>

          <button
            className="secondary"
            onClick={refreshDevices}
            disabled={status.connected}
          >
            Rescan
          </button>

          {!status.connected ? (
            <button
              className="primary"
              onClick={handleConnect}
              disabled={!selectedDevice || !isWindows}
            >
              Connect
            </button>
          ) : (
            <button className="danger" onClick={handleDisconnect}>
              Disconnect
            </button>
          )}

          <div
            className={`status-indicator ${status.connected ? "connected" : "disconnected"}`}
          >
            <span
              className={`status-dot ${status.connected ? "connected" : "disconnected"}`}
            />
            {status.connected ? "Connected" : "Disconnected"}
          </div>
        </div>

        {status.connected && (
          <div className="stats-row">
            <div className="stat-item">
              <span className="label">Device:</span>
              <span className="value">{status.deviceName}</span>
            </div>
            <div className="stat-item">
              <span className="label">Baud:</span>
              <span className="value">{status.baudRate / 1000} kbps</span>
            </div>
            <div className="stat-item">
              <span className="label">TX:</span>
              <span className="value">{status.messagesSent}</span>
            </div>
            <div className="stat-item">
              <span className="label">RX:</span>
              <span className="value">{status.messagesReceived}</span>
            </div>
          </div>
        )}
      </div>

      {/* Tabs */}
      <div className="tabs">
        <button
          className={`tab ${activeTab === "messages" ? "active" : ""}`}
          onClick={() => setActiveTab("messages")}
        >
          Messages
        </button>
        <button
          className={`tab ${activeTab === "device-info" ? "active" : ""}`}
          onClick={() => setActiveTab("device-info")}
          disabled={!status.connected}
        >
          Device Info
        </button>
        <button
          className={`tab ${activeTab === "api-test" ? "active" : ""}`}
          onClick={() => setActiveTab("api-test")}
          disabled={!status.connected}
        >
          API Testing
        </button>
      </div>

      {/* Messages Tab */}
      {activeTab === "messages" && (
        <>
          {/* Message Send Panel */}
          <div className="message-panel">
            <h2>Send Message</h2>

            <div className="message-input-row">
              <div className="form-group">
                <label>Arbitration ID (hex)</label>
                <input
                  type="text"
                  value={txArbId}
                  onChange={(e) => setTxArbId(e.target.value)}
                  placeholder="7E0"
                  disabled={!status.connected}
                  style={{ width: "100px" }}
                />
              </div>

              <div className="form-group">
                <label>Data (hex bytes)</label>
                <input
                  type="text"
                  value={txData}
                  onChange={(e) => setTxData(e.target.value)}
                  placeholder="01 00"
                  disabled={!status.connected}
                  style={{ width: "250px" }}
                />
              </div>

              <div className="checkbox-group">
                <input
                  type="checkbox"
                  id="txExtended"
                  checked={txExtended}
                  onChange={(e) => setTxExtended(e.target.checked)}
                  disabled={!status.connected}
                />
                <label htmlFor="txExtended">Extended ID</label>
              </div>

              <button
                className="primary"
                onClick={handleSendMessage}
                disabled={!status.connected}
              >
                Send
              </button>

              <button
                className="secondary"
                onClick={handleClearBuffers}
                disabled={!status.connected}
              >
                Clear Buffers
              </button>
            </div>
          </div>

          {/* Message Log */}
          <div className="message-log">
            <h2>
              Message Log
              <div className="log-controls">
                <label style={{ fontSize: "13px", display: "flex", gap: "6px" }}>
                  <input
                    type="checkbox"
                    checked={autoScroll}
                    onChange={(e) => setAutoScroll(e.target.checked)}
                  />
                  Auto-scroll
                </label>
                <button className="secondary" onClick={handleClearLog}>
                  Clear
                </button>
              </div>
            </h2>

            <div className="log-container" ref={logContainerRef}>
              {logEntries.length === 0 ? (
                <div className="empty-state">No messages yet</div>
              ) : (
                logEntries.map((entry) => (
                  <div
                    key={entry.id}
                    className={`log-entry ${entry.direction === "error" ? "error" : entry.direction === "info" ? "info" : ""}`}
                  >
                    <span className="timestamp">
                      {formatTimestamp(entry.timestamp)}
                    </span>
                    {entry.direction === "error" || entry.direction === "info" ? (
                      <span className="data">{entry.message}</span>
                    ) : (
                      <>
                        <span className={`direction ${entry.direction}`}>
                          {entry.direction === "tx" ? "TX" : "RX"}
                        </span>
                        <span className="arb-id">
                          {entry.extended ? "X" : "S"}{" "}
                          {formatHex(entry.arbId!, entry.extended ? 8 : 3)}
                        </span>
                        <span className="data">
                          [{entry.data!.length}] {formatData(entry.data!)}
                        </span>
                      </>
                    )}
                  </div>
                ))
              )}
            </div>
          </div>
        </>
      )}

      {/* Device Info Tab */}
      {activeTab === "device-info" && status.connected && (
        <div className="device-info-panel">
          <h2>Device Information</h2>

          <div className="info-grid">
            <div className="info-section">
              <h3>Version Information</h3>
              <div className="info-item">
                <span className="label">Firmware Version:</span>
                <span className="value">{versionInfo?.firmwareVersion || "N/A"}</span>
              </div>
              <div className="info-item">
                <span className="label">DLL Version:</span>
                <span className="value">{versionInfo?.dllVersion || "N/A"}</span>
              </div>
              <div className="info-item">
                <span className="label">API Version:</span>
                <span className="value">{versionInfo?.apiVersion || "N/A"}</span>
              </div>
            </div>

            <div className="info-section">
              <h3>Voltage Readings</h3>
              <div className="info-item">
                <span className="label">Battery Voltage:</span>
                <span className="value">
                  {batteryVoltage !== null ? `${batteryVoltage.toFixed(2)} V` : "N/A"}
                </span>
              </div>
              <div className="info-item">
                <span className="label">Programming Voltage:</span>
                <span className="value">
                  {progVoltage !== null ? `${progVoltage.toFixed(2)} V` : "N/A"}
                </span>
              </div>
            </div>

            <div className="info-section">
              <h3>Configuration</h3>
              <div className="info-item">
                <span className="label">Data Rate:</span>
                <span className="value">
                  {currentDataRate !== null ? `${currentDataRate} bps` : "N/A"}
                </span>
              </div>
              <div className="info-item">
                <span className="label">Loopback:</span>
                <span className="value">
                  {loopbackEnabled !== null ? (loopbackEnabled ? "Enabled" : "Disabled") : "N/A"}
                </span>
                <button
                  className="secondary small"
                  onClick={handleToggleLoopback}
                  disabled={loopbackEnabled === null}
                >
                  Toggle
                </button>
              </div>
            </div>
          </div>

          <button className="secondary" onClick={fetchDeviceInfo}>
            Refresh
          </button>
        </div>
      )}

      {/* API Testing Tab */}
      {activeTab === "api-test" && status.connected && (
        <div className="api-test-panel">
          <div className="api-test-grid">
            {/* Left column: Test controls */}
            <div className="test-controls">
              {/* Basic Functions */}
              <div className="test-section">
                <h3>Basic Functions</h3>
                <div className="button-row">
                  <button className="api-btn" onClick={handleGetLastError}>
                    GetLastError
                  </button>
                  <button className="api-btn" onClick={handleClearBuffers}>
                    Clear Buffers
                  </button>
                  <button className="api-btn" onClick={handleReadVersion}>
                    Read Version
                  </button>
                </div>
              </div>

              {/* Periodic Messages */}
              <div className="test-section">
                <h3>Periodic Messages</h3>
                <div className="form-row">
                  <div className="form-group small">
                    <label>Arb ID</label>
                    <input
                      type="text"
                      value={periodicArbId}
                      onChange={(e) => setPeriodicArbId(e.target.value)}
                      placeholder="7DF"
                    />
                  </div>
                  <div className="form-group">
                    <label>Data (hex)</label>
                    <input
                      type="text"
                      value={periodicData}
                      onChange={(e) => setPeriodicData(e.target.value)}
                      placeholder="02 01 00"
                    />
                  </div>
                  <div className="form-group small">
                    <label>Interval (ms)</label>
                    <input
                      type="number"
                      value={periodicInterval}
                      onChange={(e) => setPeriodicInterval(Number(e.target.value))}
                      min={5}
                      max={65535}
                    />
                  </div>
                  <div className="checkbox-group">
                    <input
                      type="checkbox"
                      id="periodicExt"
                      checked={periodicExtended}
                      onChange={(e) => setPeriodicExtended(e.target.checked)}
                    />
                    <label htmlFor="periodicExt">Ext</label>
                  </div>
                </div>
                <div className="button-row">
                  <button className="api-btn primary" onClick={handleStartPeriodicMessage}>
                    Start Periodic
                  </button>
                  <button className="api-btn danger" onClick={handleClearPeriodicMessages}>
                    Clear All
                  </button>
                </div>
                {activePeriodicMsgs.length > 0 && (
                  <div className="active-items">
                    <h4>Active Periodic Messages</h4>
                    {activePeriodicMsgs.map((msg) => (
                      <div key={msg.msgId} className="active-item">
                        <span>
                          ID:{msg.msgId} | 0x{formatHex(msg.arbId, 3)} | {formatData(msg.data)} | {msg.intervalMs}ms
                        </span>
                        <button
                          className="small danger"
                          onClick={() => handleStopPeriodicMessage(msg.msgId)}
                        >
                          Stop
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {/* Message Filters */}
              <div className="test-section">
                <h3>Message Filters</h3>
                <div className="form-row">
                  <div className="form-group">
                    <label>Type</label>
                    <select
                      value={filterType}
                      onChange={(e) => setFilterType(e.target.value as "pass" | "block" | "flow_control")}
                    >
                      <option value="pass">Pass</option>
                      <option value="block">Block</option>
                      <option value="flow_control">Flow Control</option>
                    </select>
                  </div>
                  <div className="form-group">
                    <label>Mask (4 bytes hex)</label>
                    <input
                      type="text"
                      value={filterMask}
                      onChange={(e) => setFilterMask(e.target.value)}
                      placeholder="00 00 00 00"
                    />
                  </div>
                  <div className="form-group">
                    <label>Pattern (4 bytes hex)</label>
                    <input
                      type="text"
                      value={filterPattern}
                      onChange={(e) => setFilterPattern(e.target.value)}
                      placeholder="00 00 00 00"
                    />
                  </div>
                  <div className="checkbox-group">
                    <input
                      type="checkbox"
                      id="filterExt"
                      checked={filterExtended}
                      onChange={(e) => setFilterExtended(e.target.checked)}
                    />
                    <label htmlFor="filterExt">Ext</label>
                  </div>
                </div>
                <div className="button-row">
                  <button className="api-btn primary" onClick={handleAddFilter}>
                    Add Filter
                  </button>
                  <button className="api-btn danger" onClick={handleClearFilters}>
                    Clear All
                  </button>
                </div>
                {activeFilters.length > 0 && (
                  <div className="active-items">
                    <h4>Active Filters</h4>
                    {activeFilters.map((filter) => (
                      <div key={filter.filterId} className="active-item">
                        <span>
                          ID:{filter.filterId} | {filter.filterType} | M:{formatData(filter.mask)} | P:{formatData(filter.pattern)}
                        </span>
                        <button
                          className="small danger"
                          onClick={() => handleRemoveFilter(filter.filterId)}
                        >
                          Remove
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {/* Configuration Parameters */}
              <div className="test-section">
                <h3>Configuration Parameters</h3>
                <div className="form-row">
                  <div className="form-group">
                    <label>Parameter</label>
                    <select
                      value={configParam}
                      onChange={(e) => setConfigParam(Number(e.target.value))}
                    >
                      {Object.entries(J2534ConfigParams).map(([name, id]) => (
                        <option key={id} value={id}>
                          {name} (0x{id.toString(16).toUpperCase()})
                        </option>
                      ))}
                    </select>
                  </div>
                  <div className="form-group small">
                    <label>Value</label>
                    <input
                      type="text"
                      value={configValue}
                      onChange={(e) => setConfigValue(e.target.value)}
                      placeholder="0"
                    />
                  </div>
                </div>
                <div className="button-row">
                  <button className="api-btn" onClick={handleGetConfig}>
                    Get Config
                  </button>
                  <button className="api-btn primary" onClick={handleSetConfig}>
                    Set Config
                  </button>
                </div>
                {configReadValue !== null && (
                  <div className="config-result">
                    Current value: <strong>{configReadValue}</strong>
                  </div>
                )}
              </div>
            </div>

            {/* Right column: Test results */}
            <div className="test-results">
              <h3>Test Results</h3>
              <div className="results-container">
                {testResults.length === 0 ? (
                  <div className="empty-state">No test results yet</div>
                ) : (
                  testResults.map((result, idx) => (
                    <div
                      key={`${result.timestamp}-${idx}`}
                      className={`result-entry ${result.success ? "success" : "error"}`}
                    >
                      <span className="result-icon">
                        {result.success ? "\u2713" : "\u2717"}
                      </span>
                      <span className="result-name">{result.name}</span>
                      <span className="result-message">{result.message}</span>
                    </div>
                  ))
                )}
              </div>
              {testResults.length > 0 && (
                <button
                  className="secondary"
                  onClick={() => setTestResults([])}
                >
                  Clear Results
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Progress Dialog */}
      {showProgress && (
        <div className="progress-overlay">
          <div className="progress-dialog">
            <h3>Connecting to Device</h3>
            <div className="progress-steps">
              {["load_dll", "open_device", "connect_channel", "set_filter", "loopback", "complete"].map(
                (step) => {
                  const progress = progressSteps.find((p) => p.step === step);
                  const stepStatus = progress?.status || "pending";

                  return (
                    <div key={step} className="progress-step">
                      <div className={`step-icon ${stepStatus}`}>
                        {stepStatus === "success"
                          ? "\u2713"
                          : stepStatus === "error"
                            ? "\u2717"
                            : stepStatus === "in_progress"
                              ? "\u2022"
                              : "\u25CB"}
                      </div>
                      <div className="step-content">
                        <div className="step-name">
                          {STEP_LABELS[step] || step}
                        </div>
                        {progress?.message && (
                          <div className="step-message">{progress.message}</div>
                        )}
                      </div>
                    </div>
                  );
                }
              )}
            </div>
          </div>
        </div>
      )}

      {/* About Dialog */}
      {showAbout && (
        <div className="progress-overlay" onClick={() => setShowAbout(false)}>
          <div className="about-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="about-header">
              <h1>Jester</h1>
              <p className="about-subtitle">An opinionated J2534-DLL-Tester</p>
            </div>
            <div className="about-content">
              <p>
                Jester is a diagnostic tool for testing and validating J2534 PassThru
                devices and their DLL implementations.
              </p>
              <h4>Features</h4>
              <ul>
                <li>Connect to any J2534-compliant PassThru device</li>
                <li>Send and receive CAN messages in real-time</li>
                <li>Monitor message traffic with detailed logging</li>
                <li>Test periodic messages and message filters</li>
                <li>Read device information and voltage levels</li>
                <li>Configure protocol parameters (baud rate, loopback, etc.)</li>
                <li>Supports both standard (11-bit) and extended (29-bit) CAN IDs</li>
              </ul>
              <p className="about-note">
                Designed for automotive engineers, ECU developers, and anyone working
                with vehicle diagnostics.
              </p>
            </div>
            <div className="about-footer">
              <button className="primary" onClick={() => setShowAbout(false)}>Close</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
