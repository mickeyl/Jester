export interface J2534Device {
  name: string;
  vendor: string;
  dllPath: string;
  canIso15765: boolean;
  canIso11898: boolean;
  compatible: boolean;
  bitness: number;  // 32 or 64
  native: boolean;  // true if loaded directly, false if via bridge
}

export interface J2534Config {
  deviceName: string;
  baudRate: number;
  protocol: string;
  useExtendedId: boolean;
}

export interface CANMessage {
  timestampUs: number;
  arbId: number;
  extended: boolean;
  data: number[];
}

export interface ConnectionStatus {
  connected: boolean;
  deviceName: string;
  baudRate: number;
  messagesSent: number;
  messagesReceived: number;
}

export interface J2534Progress {
  step: string;
  status: "pending" | "in_progress" | "success" | "error";
  message?: string;
}

export interface SendMessageRequest {
  arbId: number;
  data: number[];
  extended: boolean;
}

export interface PlatformInfo {
  platform: string;
  arch: string;
}

export interface J2534VersionInfo {
  firmwareVersion: string;
  dllVersion: string;
  apiVersion: string;
}

export interface PeriodicMessageRequest {
  arbId: number;
  data: number[];
  intervalMs: number;
  extended: boolean;
}

export interface FilterRequest {
  filterType: "pass" | "block" | "flow_control";
  mask: number[];
  pattern: number[];
  extended: boolean;
}

export interface ConfigRequest {
  parameter: number;
  value?: number;
}

// J2534 Config Parameter IDs
export const J2534ConfigParams = {
  DATA_RATE: 0x01,
  LOOPBACK: 0x03,
  NODE_ADDRESS: 0x04,
  NETWORK_LINE: 0x05,
  P1_MIN: 0x06,
  P1_MAX: 0x07,
  P2_MIN: 0x08,
  P2_MAX: 0x09,
  P3_MIN: 0x0a,
  P3_MAX: 0x0b,
  P4_MIN: 0x0c,
  P4_MAX: 0x0d,
  W1: 0x0e,
  W2: 0x0f,
  W3: 0x10,
  W4: 0x11,
  W5: 0x12,
  TIDLE: 0x13,
  TINIL: 0x14,
  TWUP: 0x15,
  PARITY: 0x16,
  BIT_SAMPLE_POINT: 0x17,
  SYNC_JUMP_WIDTH: 0x18,
  W0: 0x19,
  T1_MAX: 0x1a,
  T2_MAX: 0x1b,
  T4_MAX: 0x1c,
  T5_MAX: 0x1d,
  ISO15765_BS: 0x1e,
  ISO15765_STMIN: 0x1f,
  DATA_BITS: 0x20,
  FIVE_BAUD_MOD: 0x21,
  BS_TX: 0x22,
  STMIN_TX: 0x23,
  T3_MAX: 0x24,
  ISO15765_WFT_MAX: 0x25,
} as const;

export const J2534ConfigParamNames: Record<number, string> = {
  0x01: "DATA_RATE",
  0x03: "LOOPBACK",
  0x04: "NODE_ADDRESS",
  0x05: "NETWORK_LINE",
  0x06: "P1_MIN",
  0x07: "P1_MAX",
  0x08: "P2_MIN",
  0x09: "P2_MAX",
  0x0a: "P3_MIN",
  0x0b: "P3_MAX",
  0x0c: "P4_MIN",
  0x0d: "P4_MAX",
  0x0e: "W1",
  0x0f: "W2",
  0x10: "W3",
  0x11: "W4",
  0x12: "W5",
  0x13: "TIDLE",
  0x14: "TINIL",
  0x15: "TWUP",
  0x16: "PARITY",
  0x17: "BIT_SAMPLE_POINT",
  0x18: "SYNC_JUMP_WIDTH",
  0x19: "W0",
  0x1a: "T1_MAX",
  0x1b: "T2_MAX",
  0x1c: "T4_MAX",
  0x1d: "T5_MAX",
  0x1e: "ISO15765_BS",
  0x1f: "ISO15765_STMIN",
  0x20: "DATA_BITS",
  0x21: "FIVE_BAUD_MOD",
  0x22: "BS_TX",
  0x23: "STMIN_TX",
  0x24: "T3_MAX",
  0x25: "ISO15765_WFT_MAX",
};

export interface TestResult {
  name: string;
  success: boolean;
  message: string;
  timestamp: number;
}

export interface ActivePeriodicMessage {
  msgId: number;
  arbId: number;
  data: number[];
  intervalMs: number;
}

export interface ActiveFilter {
  filterId: number;
  filterType: string;
  mask: number[];
  pattern: number[];
}
