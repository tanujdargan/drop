# Phase 1: Raspberry Pi 3B+ AWDL Setup

## Why Raspberry Pi 3B+?

Your Pi 3B+ has a **Broadcom BCM43455** WiFi chip. This is excellent because:

1. **Nexmon** - A firmware patching framework by **seemoo-lab** (same team that made OWL and OpenDrop!) enables monitor mode on this exact chip
2. The BCM43455 supports monitor mode AND has partial packet injection support
3. It's designed to work together - Nexmon → OWL → OpenDrop

---

## Hardware Setup

### What You Need
- Raspberry Pi 3B+
- MicroSD card (16GB+ recommended)
- Power supply (2.5A minimum)
- Ethernet cable (for initial setup - WiFi will be used for AWDL)
- iPhone 16 Pro (for testing)

### Recommended OS
**Raspberry Pi OS Lite (64-bit)** - Bullseye or Bookworm

```bash
# Download from: https://www.raspberrypi.com/software/operating-systems/
# Write with Raspberry Pi Imager or:
sudo dd if=raspios-bullseye-arm64-lite.img of=/dev/sdX bs=4M status=progress
```

---

## Step 1: Initial Pi Setup

### 1.1 Flash and Boot

1. Flash Raspberry Pi OS Lite (64-bit) to SD card
2. Enable SSH by creating empty `ssh` file in boot partition
3. Boot the Pi and connect via Ethernet
4. SSH in: `ssh pi@raspberrypi.local` (default password: raspberry)

### 1.2 Update System

```bash
sudo apt update && sudo apt upgrade -y
sudo reboot
```

### 1.3 Install Build Dependencies

```bash
sudo apt install -y \
    git \
    build-essential \
    bc \
    bison \
    flex \
    libssl-dev \
    libncurses5-dev \
    libelf-dev \
    raspberrypi-kernel-headers \
    qpdf \
    texinfo \
    libtool \
    autoconf \
    automake
```

---

## Step 2: Install Nexmon (Monitor Mode for BCM43455)

Nexmon patches the Broadcom firmware to enable monitor mode and frame injection.

### 2.1 Clone Nexmon

```bash
cd ~
git clone https://github.com/seemoo-lab/nexmon.git
cd nexmon
```

### 2.2 Setup Build Environment

```bash
# Source the setup script
source setup_env.sh

# Build required tools
cd buildtools/isl-0.10
./configure
make
sudo make install

cd ../mpfr-3.1.4
autoreconf -f -i
./configure
make
sudo make install

cd ../libiconv-1.16 || cd ../libiconv-1.15
./configure
make
sudo make install
```

### 2.3 Build Nexmon Firmware

```bash
cd ~/nexmon

# Determine your kernel version
KERNEL_VERSION=$(uname -r)
echo "Kernel: $KERNEL_VERSION"

# For Pi 3B+ with BCM43455
cd patches/bcm43455c0/7_45_189/nexmon

# Build the patched firmware
make

# Backup original firmware
sudo cp /lib/firmware/brcm/brcmfmac43455-sdio.bin /lib/firmware/brcm/brcmfmac43455-sdio.bin.orig

# Install patched firmware
sudo make install-firmware
```

### 2.4 Build Nexutil (Utility Tool)

```bash
cd ~/nexmon/utilities/nexutil
make
sudo make install
```

### 2.5 Reboot and Verify

```bash
sudo reboot
```

After reboot:
```bash
# Check Nexmon is loaded
nexutil -v
# Should show version info

# Check WiFi interface
iw dev
# Should show wlan0

# Test monitor mode
sudo nexutil -m2  # Enable monitor mode
iw wlan0 info
# Should show "type monitor"

# Return to managed mode
sudo nexutil -m0
```

---

## Step 3: Install OWL + OpenDrop

### 3.1 Install OWL Dependencies

```bash
sudo apt install -y \
    cmake \
    libpcap-dev \
    libev-dev \
    libnl-3-dev \
    libnl-genl-3-dev
```

### 3.2 Build OWL

```bash
cd ~
git clone https://github.com/seemoo-lab/owl.git
cd owl
mkdir build && cd build
cmake ..
make

# Test OWL
./owl --help
```

### 3.3 Install OpenDrop

```bash
# Python dependencies
sudo apt install -y python3 python3-pip python3-dev libffi-dev libssl-dev

cd ~
git clone https://github.com/seemoo-lab/opendrop.git
cd opendrop
pip3 install .

# Test OpenDrop
opendrop --help
```

---

## Step 4: Running AWDL

### 4.1 Enable Monitor Mode

```bash
# Using Nexmon
sudo nexutil -m2

# Verify
iw wlan0 info
# Should show "type monitor"
```

### 4.2 Start OWL

```bash
# Terminal 1
cd ~/owl/build
sudo ./owl -i wlan0 -v

# Expected output:
# [INFO] Started AWDL daemon
# [INFO] Interface awdl0 created
# [INFO] Scanning on channels 6, 44, 149
```

If awdl0 doesn't appear, try:
```bash
# Check for errors
dmesg | tail -20

# Make sure wlan0 is in monitor mode
iw wlan0 info

# Try with explicit channel
sudo ./owl -i wlan0 -c 6 -v
```

### 4.3 Start OpenDrop

```bash
# Terminal 2 (new SSH session)
cd ~/opendrop

# Start receiver
opendrop receive

# Expected output:
# [INFO] Advertising as "raspberrypi"
# [INFO] Waiting for incoming connections...
```

### 4.4 Send from iPhone

1. On your **iPhone 16 Pro**:
   - Go to Settings → General → AirDrop
   - Set to **"Everyone for 10 Minutes"**

2. Open Files or Photos app
   - Select a small file (start with <1MB for testing)
   - Tap Share → AirDrop

3. Look for your Pi (may appear as "Unknown" or "raspberrypi")

4. Tap to send

5. On Pi, you should see:
   ```
   [INFO] Incoming connection from XX:XX:XX:XX:XX:XX
   [INFO] Sender: iPhone
   [INFO] Receiving: photo.jpg (500 KB)
   [INFO] Transfer complete!
   ```

---

## Troubleshooting

### "awdl0 interface not created"

```bash
# Check OWL output for errors
# Common issue: packet injection not working

# Test injection capability
sudo aireplay-ng -9 wlan0
# If this fails, Nexmon injection may not be fully working
# BCM43455 injection is "iffy" - receiving might still work

# Try running OWL with verbose output
sudo ./owl -i wlan0 -vvv
```

### "No devices found on iPhone"

1. Verify awdl0 exists:
   ```bash
   ip link show awdl0
   ```

2. Check OWL is receiving frames:
   ```bash
   # Look for "peer discovered" messages in OWL output
   ```

3. Try being closer to the Pi (within 5 meters)

4. Check channels - iPhone uses 44 or 149 more often:
   ```bash
   sudo ./owl -i wlan0 -c 44 -v
   ```

### "iPhone sees device but transfer fails"

OpenDrop uses self-signed TLS certificates. Newer iOS versions may reject them.

Check OpenDrop GitHub issues for workarounds:
https://github.com/seemoo-lab/opendrop/issues

### Nexmon Build Fails

```bash
# Make sure kernel headers match
apt search raspberrypi-kernel-headers
sudo apt install raspberrypi-kernel-headers

# Check kernel version
uname -r

# Nexmon patches are version-specific
# May need to find the right patch for your kernel
ls ~/nexmon/patches/bcm43455c0/
```

---

## Alternative: Use Archer T3U Plus

If Pi 3B+ injection doesn't work reliably, try your Archer T3U Plus USB adapter.

### Archer T3U Plus (RTL8812BU)

```bash
# On the Pi (or any Linux machine)
cd ~
git clone https://github.com/morrownr/88x2bu-20210702.git
cd 88x2bu-20210702

# Build
make
sudo make install

# Load module
sudo modprobe 88x2bu

# Check interface
iw dev
# Should show wlan1 or similar

# Enable monitor mode
sudo ip link set wlan1 down
sudo iw wlan1 set monitor none
sudo ip link set wlan1 up

# Verify
iw wlan1 info
# Should show "type monitor"
```

Then use OWL with wlan1:
```bash
sudo ./owl -i wlan1 -v
```

---

## Helper Script for Pi

Create a convenient start script:

```bash
cat > ~/start-drop.sh << 'EOF'
#!/bin/bash
set -e

echo "[*] Enabling monitor mode..."
sudo nexutil -m2
sleep 1

echo "[*] Verifying monitor mode..."
iw wlan0 info | grep -q "type monitor" || { echo "Failed to enable monitor mode"; exit 1; }

echo "[*] Starting OWL..."
cd ~/owl/build
sudo ./owl -i wlan0 &
OWL_PID=$!
sleep 3

# Check awdl0
if ! ip link show awdl0 &>/dev/null; then
    echo "[!] awdl0 not created - OWL may have failed"
    kill $OWL_PID 2>/dev/null
    exit 1
fi

echo "[*] Starting OpenDrop receiver..."
echo ""
echo "======================================"
echo "  Ready to receive AirDrop!"
echo "======================================"
echo ""
echo "On your iPhone:"
echo "  1. Settings → General → AirDrop → Everyone"
echo "  2. Select a file → Share → AirDrop"
echo "  3. Select this device"
echo ""
echo "Press Ctrl+C to stop"
echo ""

opendrop receive

# Cleanup
kill $OWL_PID 2>/dev/null
sudo nexutil -m0
echo "[*] Stopped"
EOF

chmod +x ~/start-drop.sh
```

Run with:
```bash
~/start-drop.sh
```

---

## What Success Looks Like

```
Terminal 1 (OWL):
[INFO] AWDL daemon started on wlan0
[INFO] Created virtual interface awdl0
[INFO] IPv6: fe80::xxxx:xxxx:xxxx:xxxx
[INFO] Peer discovered: fe80::yyyy (iPhone 16 Pro)
[INFO] Synchronized to peer, master metric: 512

Terminal 2 (OpenDrop):
[INFO] AirDrop service registered
[INFO] Incoming connection from iPhone
[INFO] Sender: Tanuj's iPhone
[INFO] Files: document.pdf (1.2 MB)
[INFO] Accepting transfer...
[INFO] Receiving: document.pdf [████████████████████] 100%
[INFO] Saved to: ./document.pdf
```

---

## Next Steps After Success

1. **Test bidirectional** - Try `opendrop send` to send TO the iPhone
2. **Analyze traffic** - Install Wireshark with AWDL dissector
3. **Document findings** - Note any iOS version quirks
4. **Phase 2** - Start Rust reimplementation using awdl-frame-parser
