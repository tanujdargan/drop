# Phase 1: Linux AWDL Test Environment Setup

## Overview

This guide will get you receiving AirDrop files from your iPhone 16 Pro on your Lenovo Yoga 7i running Linux.

**What we're setting up:**
1. OWL - Open AWDL implementation (creates virtual `awdl0` interface)
2. OpenDrop - AirDrop protocol (handles file receiving)

---

## Hardware Requirements

### The Bad News

Your Lenovo Yoga 7i has an **Intel AX211** WiFi chip. Unfortunately:
- Intel officially doesn't support monitor mode or packet injection
- Even when forced into monitor mode, packet capture doesn't work reliably
- AWDL requires raw frame injection, which Intel chips can't do

Source: [Intel Support](https://www.intel.com/content/www/us/en/support/articles/000058933/wireless/intel-wireless-ac-products.html)

### The Solution: USB WiFi Adapter

You need an external adapter with **monitor mode + packet injection** support.

#### Recommended (Best Compatibility)

| Adapter | Chipset | Price | Notes |
|---------|---------|-------|-------|
| **Alfa AWUS036ACH** | RTL8812AU | ~$50 | Dual-band, 867Mbps, great range |
| **Alfa AWUS036ACHM** | RTL8812AU | ~$45 | Smaller, still dual-band |
| **Alfa AWUS036NHA** | AR9271 | ~$30 | 2.4GHz only, but rock-solid |

#### Budget Options

| Adapter | Chipset | Price | Notes |
|---------|---------|-------|-------|
| **Panda PAU09** | RT5572 | ~$20 | Dual-band, plug & play |
| **TP-Link TL-WN722N v1** | AR9271 | ~$15 | ⚠️ Must be v1, not v2/v3 |

**My recommendation**: Get the **Alfa AWUS036ACH** - it's the gold standard for this kind of work. Dual-band means it can work on channels 6, 44, and 149 which AWDL uses.

#### Where to Buy
- Amazon (check reviews for chipset confirmation)
- [Alfa's official store](https://www.alfa.com.tw/)
- Hak5 shop

---

## Linux Setup

### Option A: Dual Boot (Recommended)

Install Ubuntu 22.04 or 24.04 LTS alongside Windows on your Lenovo.

### Option B: Live USB (For Testing)

Use a Kali Linux live USB - it has all the WiFi tools pre-installed.

```bash
# Download Kali Linux
# https://www.kali.org/get-kali/#kali-live

# Write to USB with Rufus (Windows) or:
sudo dd if=kali-linux-2024.1-live-amd64.iso of=/dev/sdX bs=4M status=progress
```

### Option C: VM (Limited Support)

VMs can work but USB passthrough for WiFi is finicky. Not recommended for first attempt.

---

## Software Installation

### Prerequisites (Ubuntu/Debian)

```bash
#!/bin/bash
# Run this script as root or with sudo

# Update system
apt update && apt upgrade -y

# Install build tools
apt install -y build-essential git cmake

# Install OWL dependencies
apt install -y libpcap-dev libev-dev libnl-3-dev libnl-genl-3-dev

# Install OpenDrop dependencies
apt install -y python3 python3-pip python3-dev
apt install -y libssl-dev libffi-dev

# Install aircrack-ng for interface management
apt install -y aircrack-ng

# Install wireless tools
apt install -y iw wireless-tools
```

### Install OWL (AWDL Implementation)

```bash
# Clone OWL
cd ~
git clone https://github.com/seemoo-lab/owl.git
cd owl

# Build
mkdir build && cd build
cmake ..
make

# Install (optional, can run from build dir)
sudo make install
```

### Install OpenDrop (AirDrop Protocol)

```bash
# Clone OpenDrop
cd ~
git clone https://github.com/seemoo-lab/opendrop.git
cd opendrop

# Install with pip
pip3 install .

# Or install in development mode
pip3 install -e .
```

### Install WiFi Adapter Drivers (if needed)

For **Alfa AWUS036ACH** (RTL8812AU):

```bash
# Clone driver
cd ~
git clone https://github.com/aircrack-ng/rtl8812au.git
cd rtl8812au

# Build and install
make
sudo make install

# Load the module
sudo modprobe 88XXau
```

---

## Running the Test

### Step 1: Identify Your WiFi Adapter

```bash
# List wireless interfaces
iw dev

# You should see something like:
# phy#0
#   Interface wlan0  (your Intel - ignore this)
# phy#1
#   Interface wlan1  (your USB adapter - use this)
```

### Step 2: Put Adapter in Monitor Mode

```bash
# Replace wlan1 with your adapter's interface name
sudo ip link set wlan1 down
sudo iw wlan1 set monitor none
sudo ip link set wlan1 up

# Verify
iw wlan1 info
# Should show "type monitor"
```

Or use airmon-ng:

```bash
sudo airmon-ng start wlan1
# Creates wlan1mon interface
```

### Step 3: Start OWL (AWDL Daemon)

```bash
# Terminal 1
cd ~/owl/build

# Start OWL on your monitor mode interface
sudo ./owl -i wlan1mon

# You should see:
# [INFO] Started AWDL daemon
# [INFO] Created awdl0 interface
# [INFO] Listening on channels 6, 44, 149
```

OWL creates a virtual `awdl0` network interface that speaks AWDL.

### Step 4: Start OpenDrop (AirDrop Receiver)

```bash
# Terminal 2
cd ~/opendrop

# Start in receive mode
opendrop receive

# You should see:
# [INFO] Advertising service...
# [INFO] Waiting for incoming connections...
```

### Step 5: Send from iPhone

1. On your **iPhone 16 Pro**:
   - Open Files app or Photos
   - Select a file/photo
   - Tap Share → AirDrop
   - Set AirDrop to **"Everyone for 10 Minutes"** (Settings → General → AirDrop)

2. Your Linux machine should appear as a device (might show as "Unknown" or the hostname)

3. Tap to send

4. On Linux, you should see:
   ```
   [INFO] Incoming connection from XX:XX:XX:XX:XX:XX
   [INFO] Receiving file: photo.jpg (2.3 MB)
   [INFO] Transfer complete!
   ```

5. File is saved to `~/Downloads/` or current directory

---

## Troubleshooting

### "No devices found" on iPhone

1. **Check OWL is running** - Look for `awdl0` interface:
   ```bash
   ip link show awdl0
   ```

2. **Check channel hopping** - OWL should be on channels 6, 44, 149:
   ```bash
   # Watch OWL output for channel switches
   ```

3. **Check monitor mode** - Verify adapter is in monitor mode:
   ```bash
   iw wlan1mon info
   ```

4. **Check distance** - Be within ~10 meters of iPhone

5. **Check AirDrop settings** - Must be "Everyone for 10 Minutes"

### "Failed to inject frame"

Your adapter doesn't support packet injection. Try:
```bash
# Test injection capability
sudo aireplay-ng -9 wlan1mon
```

If it fails, you need a different adapter.

### "OWL crashes on startup"

Check dependencies:
```bash
ldd ~/owl/build/owl
# All libraries should be found
```

### iPhone shows device but transfer fails

OpenDrop's TLS certificate might be rejected. OpenDrop uses self-signed certs which newer iOS versions may reject. Check OpenDrop issues on GitHub for workarounds.

---

## What Success Looks Like

```
Terminal 1 (OWL):
[INFO] AWDL daemon started
[INFO] Interface awdl0 created with IPv6 fe80::xxxx:xxxx:xxxx:xxxx
[INFO] Peer discovered: fe80::yyyy:yyyy:yyyy:yyyy (iPhone)
[INFO] Synchronization established, master metric: 512

Terminal 2 (OpenDrop):
[INFO] AirDrop service registered
[INFO] Incoming connection from iPhone
[INFO] Sender: John's iPhone
[INFO] Files: vacation.jpg (4.2 MB)
[INFO] Accepting transfer...
[INFO] Receiving: vacation.jpg [████████████████████] 100%
[INFO] Saved to: ./vacation.jpg
```

---

## Next Steps After Success

Once you can receive files:

1. **Test sending** - `opendrop find` and `opendrop send`
2. **Analyze traffic** - Use Wireshark with AWDL dissector
3. **Port to Rust** - Use awdl-frame-parser to build our own implementation

---

## Quick Reference Commands

```bash
# Start monitor mode
sudo airmon-ng start wlan1

# Start OWL
sudo ~/owl/build/owl -i wlan1mon

# Start OpenDrop receive
opendrop receive

# Start OpenDrop find (see nearby devices)
opendrop find

# Start OpenDrop send
opendrop send photo.jpg

# Stop monitor mode
sudo airmon-ng stop wlan1mon

# Check AWDL interface
ip addr show awdl0

# Watch AWDL traffic (requires Wireshark)
sudo wireshark -i wlan1mon -k
```
