# Drop - Cross-Platform File Sharing

Send files to nearby devices. No app needed on sender side.

## How It Works

1. **Sender** opens a lightweight web page on their device (any browser with Web Bluetooth)
2. **Sender** selects files and sees nearby Drop receivers
3. **Receiver** gets a notification and accepts the transfer
4. Files transfer over WiFi (fast) after BLE handshake (discovery)

## Project Structure

```
drop/
├── sender-web/              # Lightweight web sender (~50KB)
│   ├── index.html
│   ├── app.js              # Web Bluetooth + file upload logic
│   └── style.css
│
├── receiver-android/        # Android receiver app
│   └── app/
│       └── src/main/java/dev/drop/receiver/
│           ├── ble/        # BLE GATT server
│           ├── http/       # HTTP file receive server
│           └── ui/         # Main activity
│
├── receiver-core/           # Rust receiver library (for desktop)
│   └── src/
│       ├── lib.rs          # Main library
│       ├── protocol.rs     # Drop protocol definitions
│       ├── ble.rs          # BLE peripheral logic
│       ├── http_server.rs  # HTTP file receive server
│       └── storage.rs      # File storage management
│
├── src/                     # Legacy signaling server (optional)
└── frontend/                # Legacy Next.js frontend (deprecated)
```

## Quick Start

### Testing: Windows Browser → Android

**Prerequisites:**
- Windows PC with Bluetooth and Chrome/Edge browser
- Android device (tablet/phone) with Bluetooth

**Step 1: Build and Install Android Receiver**

```bash
cd receiver-android
./gradlew assembleDebug
adb install app/build/outputs/apk/debug/app-debug.apk
```

**Step 2: Start Android Receiver**

1. Open the "Drop" app on your Android device
2. Grant Bluetooth and notification permissions
3. Tap "Start Receiver" - you'll see "Ready to receive files"
4. Note the IP address shown

**Step 3: Open Web Sender**

1. On Windows, open Chrome/Edge
2. Navigate to `sender-web/index.html` (or serve it locally)
3. Select files to send
4. Click "Find Nearby Devices"
5. Browser will show Bluetooth device picker - select your Android device
6. Wait for receiver to accept
7. Files transfer over WiFi

### Serving the Web Sender

For local testing, you can use any simple HTTP server:

```bash
# Python
cd sender-web && python -m http.server 8000

# Node.js (npx)
cd sender-web && npx serve

# Then open http://localhost:8000 in Chrome/Edge
```

**Note:** Web Bluetooth requires HTTPS in production, but works on localhost for testing.

## Protocol Overview

### BLE Service

```
Service UUID: 0000fe50-0000-1000-8000-00805f9b34fb

Characteristics:
├── Device Info (Read)      - 0000fe51-...
├── Transfer Request (Write) - 0000fe52-...
├── Transfer Response (Notify) - 0000fe53-...
└── Transfer Status (Write/Notify) - 0000fe54-...
```

### Transfer Flow

```
Sender (Browser)                    Receiver (Android)
     │                                     │
     │◀──── BLE Advertisement ─────────────│ (Drop service)
     │                                     │
     │─── BLE Connect + Write Request ────▶│
     │    {files: [...], totalSize: 5MB}   │
     │                              [Notification shown]
     │                              [User taps Accept]
     │◀─── BLE Notify: Accept ─────────────│
     │     {ip: "192.168.1.50", port: 53317}
     │                                     │
     │════ HTTP POST /upload (WiFi) ══════▶│
     │     [File data via multipart form]  │
     │                                     │
     │◀═══ HTTP 200 OK ════════════════════│
     │                              [File saved]
```

## Supported Platforms

### Sender (Web App)
- ✅ Chrome (Windows, macOS, Linux, Android)
- ✅ Edge (Windows)
- ✅ Opera
- ❌ Firefox (no Web Bluetooth)
- ❌ Safari (limited Web Bluetooth)

### Receiver
- ✅ Android 8.0+ (BLE peripheral + HTTP server)
- 🚧 Windows (Rust + btleplug) - in development
- 🚧 macOS (Swift + CoreBluetooth) - planned
- 🚧 iOS (Swift + CoreBluetooth) - planned
- 🚧 Linux (Rust + BlueZ) - planned

## Development

### Android Receiver

```bash
cd receiver-android
./gradlew build
```

### Rust Receiver Core (Desktop)

```bash
cd receiver-core
cargo build
cargo test
```

### Web Sender

No build step required - pure vanilla JS. Just serve the files.

## Architecture Decisions

1. **BLE for Discovery, WiFi for Transfer**
   - BLE: ~200 Kbps - 1 Mbps real-world (too slow for files)
   - WiFi: 50-500+ Mbps (fast enough for any file size)

2. **Lightweight Web Sender**
   - No frameworks, no build step
   - ~50KB total (HTML + JS + CSS)
   - Uses Web Bluetooth API for BLE scanning

3. **Native Receivers**
   - Need background BLE advertising
   - Need local HTTP server for file receiving
   - Minimal size (~2-3MB per platform)

4. **HTTP for File Transfer**
   - Simpler than WebRTC for local network
   - No STUN/TURN servers needed
   - Standard multipart form upload

## License

MIT
