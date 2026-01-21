import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { save } from "@tauri-apps/plugin-dialog";
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
  SanityReport,
  SanityStepResult,
  SanityOptions,
  SanityStatus,
  ActivePeriodicMessage,
  ActiveFilter,
  BatchTestResult,
  BatchTestSummary,
  BatchSendRequest,
  BatchSendResult,
} from "./types";
import { J2534ConfigParams, J2534ConfigParamNames } from "./types";
import type { J2534Protocol } from "./types";

interface LogEntry {
  id: number;
  timestamp: Date;
  direction: "tx" | "rx" | "error" | "info";
  arbId?: number;
  extended?: boolean;
  data?: number[];
  message?: string;
  deviceTimestampUs?: number; // J2534 device timestamp in microseconds (RX only)
}

type TabType = "messages" | "device-info" | "sanity" | "api-test" | "batch-test";

const STEP_LABELS: Record<string, string> = {
  load_dll: "Load J2534 DLL",
  open_device: "Open Device",
  connect_channel: "Connect CAN Channel",
  set_filter: "Configure Message Filter",
  loopback: "Loopback Settings",
  complete: "Connection Complete",
};

const SANITY_TEST_ARB_ID = 0x7df;
const SANITY_TEST_DATA = [0x02, 0x01, 0x00, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa];

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

function formatDateTime(ms: number): string {
  const date = new Date(ms);
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  const hours = String(date.getHours()).padStart(2, "0");
  const minutes = String(date.getMinutes()).padStart(2, "0");
  const seconds = String(date.getSeconds()).padStart(2, "0");
  return `${year}-${month}-${day} ${hours}:${minutes}:${seconds}`;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(2)} s`;
  const minutes = Math.floor(seconds / 60);
  const remainder = seconds % 60;
  return `${minutes}m ${remainder.toFixed(1)}s`;
}

function summarizeSanity(report: SanityReport) {
  return report.steps.reduce(
    (acc, step) => {
      acc.total += 1;
      acc[step.status] += 1;
      return acc;
    },
    { pass: 0, fail: 0, warn: 0, skip: 0, total: 0 }
  );
}

function sanityStatusIcon(status: SanityStatus): string {
  switch (status) {
    case "pass":
      return "\u2713";
    case "fail":
      return "\u2717";
    case "warn":
      return "\u26A0";
    case "skip":
      return "\u25CB";
    default:
      return "?";
  }
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
  // Note: Only CAN protocol is supported. Other J2534 protocols (ISO15765, ISO9141, etc.)
  // are optional in the spec, so adapter support is inconsistent and unreliable.
  const [baudRate, setBaudRate] = useState(500000);
  const [protocol] = useState<J2534Protocol>("CAN");
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

  // Sanity suite state
  const [sanityReport, setSanityReport] = useState<SanityReport | null>(null);
  const [sanityRunning, setSanityRunning] = useState(false);
  const [sanityError, setSanityError] = useState<string | null>(null);
  const [sanitySteps, setSanitySteps] = useState<SanityStepResult[]>([]);

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

  // About dialog - false by default (user can open via ? button)
  const [showAbout, setShowAbout] = useState(false);
  const [aboutAutoOpened, setAboutAutoOpened] = useState(false);

  // Show about dialog once per day, auto-close after 2.5s
  useEffect(() => {
    const lastShown = localStorage.getItem("jester-about-last-shown");
    const today = new Date().toDateString();
    if (lastShown !== today) {
      localStorage.setItem("jester-about-last-shown", today);
      setShowAbout(true);
      setAboutAutoOpened(true);
    }
  }, []);

  // Auto-close about dialog after 2.5s if auto-opened
  useEffect(() => {
    if (showAbout && aboutAutoOpened) {
      const timer = setTimeout(() => {
        setShowAbout(false);
        setAboutAutoOpened(false);
      }, 2500);
      return () => clearTimeout(timer);
    }
  }, [showAbout, aboutAutoOpened]);

  // Batch test state
  const [batchArbId, setBatchArbId] = useState("7E0");
  const [batchCount, setBatchCount] = useState(100);
  const [batchInterval, setBatchInterval] = useState(10);
  const [batchExtended, setBatchExtended] = useState(false);
  const [batchPayloadSize, setBatchPayloadSize] = useState(6);
  const [batchRunning, setBatchRunning] = useState(false);
  const [batchResults, setBatchResults] = useState<Map<number, BatchTestResult>>(new Map());
  const [batchSummary, setBatchSummary] = useState<BatchTestSummary | null>(null);
  const [batchProgress, setBatchProgress] = useState(0);
  const batchAbortRef = useRef(false);

  // Quality test protocol state
  const [useQualityTestFormat, setUseQualityTestFormat] = useState(false);
  const [qualityTestId, setQualityTestId] = useState(1);

  // Load platform info and devices on mount
  useEffect(() => {
    invoke<PlatformInfo>("get_platform_info").then(setPlatformInfo);
    refreshDevices();
  }, []);

  // Set window title with bitness
  useEffect(() => {
    if (platformInfo) {
      const bits = platformInfo.arch === "x86_64" ? "64" : "32";
      getCurrentWindow().setTitle(`Jester [${bits}-bit]`);
    }
  }, [platformInfo]);

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

  // Listen for sanity step events (progressive disclosure)
  useEffect(() => {
    const unlisten = listen<SanityStepResult>("sanity-step", (event) => {
      setSanitySteps((prev) => [...prev, event.payload]);
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
    if (status.connected && !sanityRunning && !batchRunning) {
      const poll = async () => {
        try {
          const messages = await invoke<CANMessage[]>("j2534_read_messages", {
            timeoutMs: 100,
          });

          if (messages.length > 0) {
            const now = new Date();
            setLogEntries((prev) => [
              ...prev,
              ...messages.map((msg) => ({
                id: ++logIdRef.current,
                timestamp: now,
                direction: "rx" as const,
                arbId: msg.arbId,
                extended: msg.extended,
                data: msg.data,
                deviceTimestampUs: msg.timestampUs,
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
  }, [status.connected, sanityRunning, batchRunning]);

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
        // Auto-select first device
        setSelectedDevice(deviceList[0].name);
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
        protocol,
        useExtendedId,
      };

      await invoke("j2534_connect", { config });

      const newStatus = await invoke<ConnectionStatus>("j2534_get_status");
      setStatus(newStatus);
    } catch (err) {
      setError(String(err));
      setShowProgress(false);
    }
  }, [selectedDevice, baudRate, protocol, useExtendedId]);

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

  const handleRunSanitySuite = useCallback(async () => {
    if (!status.connected) return;

    setSanityError(null);
    setSanityRunning(true);
    setSanitySteps([]);
    setSanityReport(null);

    try {
      const options: SanityOptions = {
        arbId: SANITY_TEST_ARB_ID,
        data: SANITY_TEST_DATA,
        extended: false,
        responseTimeoutMs: 800,
        periodicIntervalMs: 100,
      };

      const report = await invoke<SanityReport>("j2534_run_sanity_suite", { options });
      setSanityReport(report);
      const newStatus = await invoke<ConnectionStatus>("j2534_get_status");
      setStatus(newStatus);
    } catch (err) {
      setSanityError(String(err));
    } finally {
      setSanityRunning(false);
    }
  }, [status.connected]);

  const handleSaveSanityReport = useCallback(async () => {
    if (!sanityReport) return;

    try {
      const summary = summarizeSanity(sanityReport);
      const payload = {
        ...sanityReport,
        summary,
      };
      const defaultName = `jester-sanity-report-${formatDateTime(sanityReport.completedAt).replace(/[:\s]/g, "-")}.json`;
      const path = await save({
        defaultPath: defaultName,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path || Array.isArray(path)) return;

      await invoke("save_sanity_report", {
        path,
        contents: JSON.stringify(payload, null, 2),
      });
    } catch (err) {
      setSanityError(String(err));
    }
  }, [sanityReport]);

  // Build quality test protocol message
  const buildQualityTestMessage = useCallback((seq: number, testStartTime: number, testId: number): number[] => {
    const magic = [0xCA, 0xFE];
    const seqBytes = [(seq >> 8) & 0xFF, seq & 0xFF];
    const tsOffset = (Date.now() - testStartTime) & 0xFFFF;
    const tsBytes = [(tsOffset >> 8) & 0xFF, tsOffset & 0xFF];
    const testIdByte = testId & 0xFF;
    const payload = [...magic, ...seqBytes, ...tsBytes, testIdByte];
    const checksum = payload.reduce((acc, b) => acc ^ b, 0);
    return [...payload, checksum];
  }, []);

  // Batch test handlers
  const handleRunBatchTest = useCallback(async () => {
    if (!status.connected || batchRunning) return;

    const arbId = parseInt(batchArbId, 16);
    if (isNaN(arbId)) {
      setError("Invalid arbitration ID");
      return;
    }

    // Only enable loopback for legacy mode (not quality test protocol)
    // Quality test protocol: Jester sends -> CANcorder receives
    if (!useQualityTestFormat) {
      try {
        await invoke("j2534_set_loopback", { enabled: true });
      } catch (err) {
        setError(`Failed to enable loopback: ${err}`);
        return;
      }
    }

    setError(null);
    setBatchRunning(true);
    setBatchResults(new Map());
    setBatchSummary(null);
    setBatchProgress(0);
    batchAbortRef.current = false;

    const results = new Map<number, BatchTestResult>();
    const testStartTime = Date.now();

    // Build all messages upfront
    const messages: SendMessageRequest[] = [];
    for (let seq = 0; seq < batchCount; seq++) {
      let data: number[];

      if (useQualityTestFormat) {
        // Quality test protocol: 8-byte fixed format with magic marker
        data = buildQualityTestMessage(seq, testStartTime, qualityTestId);
      } else {
        // Legacy format: 2-byte sequence number (big endian) + padding
        data = [
          (seq >> 8) & 0xff,
          seq & 0xff,
        ];
        // Add padding bytes
        for (let i = 0; i < batchPayloadSize; i++) {
          data.push((seq + i) & 0xff);
        }
      }

      messages.push({
        arbId,
        data,
        extended: batchExtended,
      });

      // Initialize result tracking
      results.set(seq, {
        sequenceNumber: seq,
        sent: false,
        received: false,
      });
    }

    setBatchProgress(10);

    // Send ALL messages in a single PassThruWriteMsgs call
    let numSent = 0;
    const sendTime = Date.now();
    try {
      const request: BatchSendRequest = { messages };
      const result = await invoke<BatchSendResult>("j2534_send_messages_batch", { request });
      numSent = result.sent;

      // Mark sent messages
      for (let seq = 0; seq < numSent; seq++) {
        const r = results.get(seq);
        if (r) {
          r.sent = true;
          r.sentAt = sendTime;
          // For quality test protocol, mark as "received" since CANcorder handles reception
          if (useQualityTestFormat) {
            r.received = true;
          }
          results.set(seq, r);
        }
      }

      console.log(`Batch send: requested=${result.requested}, sent=${result.sent}`);
    } catch (err) {
      setError(`Batch send failed: ${err}`);
      console.error("Batch send error:", err);
    }

    setBatchProgress(50);

    // For quality test protocol, skip loopback polling - CANcorder receives the packets
    if (!useQualityTestFormat) {
      // Wait for responses - poll for a bit after sending
      const pollEndTime = Date.now() + Math.max(1000, batchCount * 5);

      while (Date.now() < pollEndTime && !batchAbortRef.current) {
          try {
            const messages = await invoke<CANMessage[]>("j2534_read_messages_with_loopback", {
              timeoutMs: 50,
            });

          for (const msg of messages) {
            // Check if this is one of our messages (loopback)
            if (msg.arbId === arbId && msg.data.length >= 2) {
              const seq = (msg.data[0] << 8) | msg.data[1];
              if (seq >= 0 && seq < batchCount) {
                const existing = results.get(seq);
                if (existing && !existing.received) {
                  existing.received = true;
                  existing.receivedAt = Date.now();
                  if (existing.sentAt) {
                    existing.roundTripMs = existing.receivedAt - existing.sentAt;
                  }
                  results.set(seq, existing);
                }
              }
            }
          }
        } catch (err) {
          // Ignore buffer empty errors
          const errStr = String(err);
          if (!errStr.includes("ERR_BUFFER_EMPTY")) {
            console.error("Read error during batch:", err);
          }
        }

        setBatchProgress(50 + ((Date.now() - testStartTime) / (pollEndTime - testStartTime + batchCount * batchInterval)) * 50);
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
    }

    // Calculate summary
    let totalSent = 0;
    let totalReceived = 0;
    const roundTrips: number[] = [];

    results.forEach((r) => {
      if (r.sent) totalSent++;
      if (r.received) totalReceived++;
      if (r.roundTripMs !== undefined) roundTrips.push(r.roundTripMs);
    });

    const summary: BatchTestSummary = {
      totalSent,
      totalReceived,
      lostCount: totalSent - totalReceived,
      lossPercent: totalSent > 0 ? ((totalSent - totalReceived) / totalSent) * 100 : 0,
    };

    if (roundTrips.length > 0) {
      summary.minRoundTripMs = Math.min(...roundTrips);
      summary.maxRoundTripMs = Math.max(...roundTrips);
      summary.avgRoundTripMs = roundTrips.reduce((a, b) => a + b, 0) / roundTrips.length;
    }

    setBatchResults(new Map(results));
    setBatchSummary(summary);
    setBatchProgress(100);
    setBatchRunning(false);
  }, [status.connected, batchRunning, batchArbId, batchCount, batchInterval, batchExtended, batchPayloadSize, useQualityTestFormat, qualityTestId, buildQualityTestMessage]);

  const handleAbortBatchTest = useCallback(() => {
    batchAbortRef.current = true;
  }, []);

  const handleClearBatchResults = useCallback(() => {
    setBatchResults(new Map());
    setBatchSummary(null);
    setBatchProgress(0);
  }, []);

  const isWindows = platformInfo?.platform === "windows";
  const sanitySummary = sanityReport ? summarizeSanity(sanityReport) : null;

  return (
    <div className="app-container">
      {/* Connection Panel */}
      <div className="connection-panel">
        <div className="panel-header">
          <h2>J2534 Connection</h2>
          <button className="about-btn" onClick={() => { setShowAbout(true); setAboutAutoOpened(false); }} title="About Jester">?</button>
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
                >
                  {device.name} ({device.vendor}) [{device.bitness}-bit{device.native ? "" : ", bridged"}]
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

      {/* Tabs - only show when connected */}
      {status.connected && (
        <>
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
            >
              Device Info
            </button>
            <button
              className={`tab ${activeTab === "sanity" ? "active" : ""}`}
              onClick={() => setActiveTab("sanity")}
            >
              Sanity Tests
            </button>
            <button
              className={`tab ${activeTab === "api-test" ? "active" : ""}`}
              onClick={() => setActiveTab("api-test")}
            >
              API Testing
            </button>
            <button
              className={`tab ${activeTab === "batch-test" ? "active" : ""}`}
              onClick={() => setActiveTab("batch-test")}
            >
              Batch Test
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
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && status.connected) {
                      handleSendMessage();
                    }
                  }}
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
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && status.connected) {
                      handleSendMessage();
                    }
                  }}
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
                        {entry.deviceTimestampUs !== undefined && (
                          <span className="device-timestamp">
                            dt:{(entry.deviceTimestampUs / 1000).toFixed(1)}ms
                          </span>
                        )}
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
      {activeTab === "device-info" && (
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

      {/* Sanity Tests Tab */}
      {activeTab === "sanity" && (
        <div className="sanity-panel">
          <div className="sanity-header">
            <div>
              <h2>Sanity Suite</h2>
              <p className="sanity-note">
                Fully-automated checks for core J2534 API calls. Tests loopback echo (guaranteed
                if driver supports it) and bus response (depends on connected ECU).
              </p>
              <p className="sanity-note">
                Uses broadcast ID 0x7DF with OBD-II PID request. Missing loopback support is reported as a warning.
              </p>
            </div>
            <div className="sanity-actions">
              <button
                className="primary"
                onClick={handleRunSanitySuite}
                disabled={sanityRunning || !status.connected}
              >
                {sanityRunning ? "Running..." : "Run Suite"}
              </button>
              <button
                className="secondary"
                onClick={handleSaveSanityReport}
                disabled={!sanityReport}
              >
                Save Report
              </button>
            </div>
          </div>

          {sanityError && <div className="error-message">{sanityError}</div>}

          {sanityReport ? (
            <>
              <div className="sanity-summary">
                <div className="summary-card">
                  <div className="summary-label">Last Run</div>
                  <div className="summary-value">{formatDateTime(sanityReport.completedAt)}</div>
                </div>
                <div className="summary-card">
                  <div className="summary-label">Duration</div>
                  <div className="summary-value">{formatDuration(sanityReport.durationMs)}</div>
                </div>
                <div className="summary-card">
                  <div className="summary-label">Results</div>
                  <div className="summary-value">
                    {sanitySummary?.pass ?? 0}P / {sanitySummary?.fail ?? 0}F / {sanitySummary?.warn ?? 0}W / {sanitySummary?.skip ?? 0}S
                  </div>
                </div>
                <div className="summary-card">
                  <div className="summary-label">Device</div>
                  <div className="summary-value">{sanityReport.deviceName || "N/A"}</div>
                  <div className="summary-subvalue">
                    {sanityReport.baudRate / 1000} kbps | Extended {sanityReport.useExtendedId ? "On" : "Off"}
                  </div>
                </div>
              </div>

              <div className="sanity-results">
                {sanityReport.steps.map((step, idx) => (
                  <div key={`${step.name}-${idx}`} className={`result-entry ${step.status}`}>
                    <span className="result-icon">{sanityStatusIcon(step.status)}</span>
                    <span className="result-name">{step.name}</span>
                    <span className="result-message">{step.message}</span>
                    <span className="result-duration">{step.durationMs}ms</span>
                  </div>
                ))}
              </div>
            </>
          ) : sanitySteps.length > 0 ? (
            <div className="sanity-results">
              {sanitySteps.map((step, idx) => (
                <div key={`${step.name}-${idx}`} className={`result-entry ${step.status}`}>
                  <span className="result-icon">{sanityStatusIcon(step.status)}</span>
                  <span className="result-name">{step.name}</span>
                  <span className="result-message">{step.message}</span>
                  <span className="result-duration">{step.durationMs}ms</span>
                </div>
              ))}
            </div>
          ) : (
            <div className="empty-state">No sanity report yet</div>
          )}
        </div>
      )}

      {/* API Testing Tab */}
      {activeTab === "api-test" && (
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

      {/* Batch Test Tab */}
      {activeTab === "batch-test" && (
        <div className="batch-test-panel">
          <div className="batch-test-header">
            <div>
              <h2>Batch Message Test</h2>
              <p className="batch-note">
                Send a configurable number of messages with sequence numbers to verify message
                delivery reliability. Uses loopback mode to track which messages are received back.
              </p>
            </div>
          </div>

          <div className="batch-test-config">
            <h3>Configuration</h3>
            <div className="batch-form-grid">
              <div className="form-group">
                <label>Base Arb ID (hex)</label>
                <input
                  type="text"
                  value={batchArbId}
                  onChange={(e) => setBatchArbId(e.target.value)}
                  placeholder="7E0"
                  disabled={batchRunning}
                  style={{ width: "100px" }}
                />
              </div>
              <div className="form-group">
                <label>Message Count</label>
                <input
                  type="number"
                  value={batchCount}
                  onChange={(e) => setBatchCount(Math.max(1, Math.min(65535, Number(e.target.value))))}
                  min={1}
                  max={65535}
                  disabled={batchRunning}
                  style={{ width: "100px" }}
                />
              </div>
              <div className="form-group">
                <label>Interval (ms)</label>
                <input
                  type="number"
                  value={batchInterval}
                  onChange={(e) => setBatchInterval(Math.max(0, Math.min(1000, Number(e.target.value))))}
                  min={0}
                  max={1000}
                  disabled={batchRunning}
                  style={{ width: "80px" }}
                />
              </div>
              <div className="form-group">
                <label>Payload Size</label>
                <select
                  value={batchPayloadSize}
                  onChange={(e) => setBatchPayloadSize(Number(e.target.value))}
                  disabled={batchRunning || useQualityTestFormat}
                  title={useQualityTestFormat ? "Fixed at 8 bytes when using Quality Test Protocol" : ""}
                >
                  <option value={1}>3 bytes (seq + 1)</option>
                  <option value={2}>4 bytes (seq + 2)</option>
                  <option value={4}>6 bytes (seq + 4)</option>
                  <option value={6}>8 bytes (seq + 6)</option>
                </select>
              </div>
              <div className="checkbox-group">
                <input
                  type="checkbox"
                  id="batchExtended"
                  checked={batchExtended}
                  onChange={(e) => setBatchExtended(e.target.checked)}
                  disabled={batchRunning}
                />
                <label htmlFor="batchExtended">Extended ID (29-bit)</label>
              </div>
            </div>

            <div className="batch-form-grid" style={{ marginTop: "12px" }}>
              <div className="checkbox-group">
                <input
                  type="checkbox"
                  id="useQualityTestFormat"
                  checked={useQualityTestFormat}
                  onChange={(e) => setUseQualityTestFormat(e.target.checked)}
                  disabled={batchRunning}
                />
                <label htmlFor="useQualityTestFormat">Use Quality Test Protocol</label>
              </div>
              {useQualityTestFormat && (
                <div className="form-group">
                  <label>Test ID (0-255)</label>
                  <input
                    type="number"
                    value={qualityTestId}
                    onChange={(e) => setQualityTestId(Math.max(0, Math.min(255, Number(e.target.value))))}
                    min={0}
                    max={255}
                    disabled={batchRunning}
                    style={{ width: "80px" }}
                  />
                </div>
              )}
            </div>
            {useQualityTestFormat && (
              <p className="batch-note" style={{ marginTop: "8px", fontSize: "12px", color: "#888" }}>
                Quality Test Protocol uses 8-byte packets: Magic (0xCAFE) + Seq (2B) + Timestamp (2B) + TestID (1B) + Checksum (1B)
              </p>
            )}

            <div className="batch-actions">
              {!batchRunning ? (
                <button
                  className="primary"
                  onClick={handleRunBatchTest}
                  disabled={!status.connected}
                >
                  Run Batch Test
                </button>
              ) : (
                <button className="danger" onClick={handleAbortBatchTest}>
                  Abort
                </button>
              )}
              <button
                className="secondary"
                onClick={handleClearBatchResults}
                disabled={batchRunning || batchResults.size === 0}
              >
                Clear Results
              </button>
            </div>

            {batchRunning && (
              <div className="batch-progress">
                <div className="progress-bar">
                  <div
                    className="progress-fill"
                    style={{ width: `${batchProgress}%` }}
                  />
                </div>
                <span className="progress-text">{Math.round(batchProgress)}%</span>
              </div>
            )}
          </div>

          {batchSummary && (
            <div className="batch-summary">
              <h3>Results Summary</h3>
              <div className="summary-grid">
                <div className={`summary-card ${batchSummary.lostCount === 0 ? "success" : "warning"}`}>
                  <div className="summary-label">Messages Sent</div>
                  <div className="summary-value">{batchSummary.totalSent}</div>
                </div>
                <div className={`summary-card ${batchSummary.lostCount === 0 ? "success" : "warning"}`}>
                  <div className="summary-label">Messages Received</div>
                  <div className="summary-value">{batchSummary.totalReceived}</div>
                </div>
                <div className={`summary-card ${batchSummary.lostCount === 0 ? "success" : "error"}`}>
                  <div className="summary-label">Lost</div>
                  <div className="summary-value">
                    {batchSummary.lostCount} ({batchSummary.lossPercent.toFixed(1)}%)
                  </div>
                </div>
                {batchSummary.avgRoundTripMs !== undefined && (
                  <div className="summary-card">
                    <div className="summary-label">Round Trip (ms)</div>
                    <div className="summary-value">
                      {batchSummary.minRoundTripMs?.toFixed(0)} / {batchSummary.avgRoundTripMs.toFixed(1)} / {batchSummary.maxRoundTripMs?.toFixed(0)}
                    </div>
                    <div className="summary-subvalue">min / avg / max</div>
                  </div>
                )}
              </div>
            </div>
          )}

          {batchResults.size > 0 && (
            <div className="batch-results">
              <h3>Sequence Details</h3>
              <div className="sequence-grid">
                {Array.from(batchResults.entries()).map(([seq, result]) => (
                  <div
                    key={seq}
                    className={`sequence-cell ${result.received ? "received" : result.sent ? "lost" : "failed"}`}
                    title={`Seq ${seq}: ${result.received ? "OK" : result.sent ? "Lost" : "Send Failed"}${result.roundTripMs ? ` (${result.roundTripMs}ms)` : ""}`}
                  >
                    {seq}
                  </div>
                ))}
              </div>
              <div className="sequence-legend">
                <span className="legend-item"><span className="legend-color received" /> Received</span>
                <span className="legend-item"><span className="legend-color lost" /> Lost</span>
                <span className="legend-item"><span className="legend-color failed" /> Send Failed</span>
              </div>
            </div>
          )}
        </div>
      )}
        </>
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
              <h4>Key Features</h4>
              <ul>
                <li><strong>Fault-Isolated Architecture</strong> &ndash; DLLs run in a separate process; crashes won't bring down the app</li>
                <li><strong>Cross-Bitness Support</strong> &ndash; 64-bit app loads 32-bit DLLs seamlessly (and vice versa)</li>
                <li>Send and receive CAN messages in real-time</li>
                <li>Test periodic messages, filters, and configuration parameters</li>
                <li>Read device information and voltage levels</li>
                <li>Supports standard (11-bit) and extended (29-bit) CAN IDs</li>
              </ul>
              <p className="about-note">
                Designed for automotive engineers, ECU developers, and anyone working
                with vehicle diagnostics.
              </p>
            </div>
            {!aboutAutoOpened && (
              <div className="about-footer">
                <button className="primary" onClick={() => setShowAbout(false)}>Close</button>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
