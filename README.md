# Jester

**An opinionated J2534-DLL-Tester**

Jester is a diagnostic tool for testing and validating J2534 PassThru devices and their DLL implementations. Built with [Tauri](https://tauri.app/) and React.

![Jester Screenshot](screenshot.png)

## Features

- Connect to any J2534-compliant PassThru device
- Send and receive CAN messages in real-time
- Monitor message traffic with detailed logging
- Test periodic messages and message filters
- Read device information and voltage levels
- Configure protocol parameters (baud rate, loopback, etc.)
- Supports both standard (11-bit) and extended (29-bit) CAN IDs
- Works with USB and WiFi-based J2534 adapters

## Requirements

- Windows 10/11 (J2534 is a Windows-only standard)
- A J2534-compliant PassThru device with its driver installed

## Installation

Download the latest release from the [Releases](https://github.com/mickeyl/Jester/releases) page.

## Building from Source

### Prerequisites

- [Node.js](https://nodejs.org/) (v18 or later)
- [Rust](https://rustup.rs/) (latest stable)
- [Tauri CLI](https://tauri.app/v1/guides/getting-started/prerequisites)

### Build Steps

```bash
# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Usage

1. Launch Jester
2. Select your J2534 device from the dropdown
3. Choose the appropriate baud rate for your vehicle/ECU
4. Click **Connect**
5. Use the **Messages** tab to send and receive CAN frames
6. Use **Device Info** to view adapter details and voltage readings
7. Use **API Testing** to test periodic messages, filters, and configuration parameters

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
