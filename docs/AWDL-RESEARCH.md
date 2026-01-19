# AWDL Research: Path to AirDrop Interoperability

## Executive Summary

**Goal**: Make Drop appear as an AirDrop device to iPhones, achieving "no install" receiving on iOS.

**Key Finding**: Google achieved this in November 2025 with Quick Share on Pixel 10. They reverse-engineered AWDL without Apple's help, implemented it in Rust for security, and it works bidirectionally with AirDrop's "Everyone for 10 minutes" mode.

**Existing Resources**:
- [OWL](https://github.com/seemoo-lab/owl) - Open AWDL implementation in C for Linux
- [OpenDrop](https://github.com/seemoo-lab/opendrop) - AirDrop protocol implementation in Python
- [awdl-frame-parser](https://github.com/Frostie314159/awdl-frame-parser) - Rust parser for AWDL frames
- [PrivateDrop](https://github.com/seemoo-lab/privatedrop) - Secure AirDrop variant

---

## How AWDL Works

### The Stack

```
┌─────────────────────────────────────────┐
│           AirDrop Application           │
│  (HTTPS/TLS 1.2 + mDNS + File Transfer) │
├─────────────────────────────────────────┤
│              IPv6 (Link-Local)          │
├─────────────────────────────────────────┤
│           AWDL Data Frames              │
│     (802.11 data + AWDL header)         │
├─────────────────────────────────────────┤
│     AWDL Control (Action Frames)        │
│   (Sync, Election, Service Discovery)   │
├─────────────────────────────────────────┤
│       BLE Advertisements (Discovery)    │
├─────────────────────────────────────────┤
│          IEEE 802.11 (WiFi PHY)         │
└─────────────────────────────────────────┘
```

### Key Components

#### 1. BLE Discovery Layer
- iPhone broadcasts BLE advertisements when AirDrop sharing pane is open
- Contains truncated Apple ID hashes (privacy vulnerability documented by PrivateDrop)
- Nearby devices see the BLE ad and wake their AWDL stack

#### 2. AWDL Synchronization
- **Availability Windows (AWs)**: 16 TU (16.384ms) time slots
- **Extended AW**: 64 TUs = ~65ms, repeating every ~1 second
- All devices sync to a master node's clock
- Allows switching between infrastructure WiFi and AWDL without dropping connection

#### 3. Channel Hopping
- Social channels: 6, 44, 149
- Devices hop between channels in sync
- Can maintain AP connection while doing AWDL

#### 4. Master Election
- Metric-based: highest metric wins
- New nodes listen for 2 seconds, then either join existing cluster or self-elect
- Seamless re-election when master leaves (~1.5 seconds)

#### 5. AirDrop Protocol (over AWDL)
- mDNS for service discovery (`_airdrop._tcp`)
- HTTPS endpoints: `/Discover`, `/Ask`, `/Upload`
- TLS 1.2 with client certificates (Apple-signed)
- File transfer via HTTP POST

---

## AWDL Frame Formats

### Action Frames (Control)

Two types:
1. **PSF (Periodic Synchronization Frame)** - subtype 0
2. **MIF (Master Indication Frame)** - subtype 3

```
┌──────────────────────────────────────────────────────────┐
│ Fixed Header (20 bytes)                                  │
├──────────────────────────────────────────────────────────┤
│ - PHY Tx Time (4 bytes) - transmission timestamp         │
│ - Target Tx Time (4 bytes) - intended tx time            │
│ - AWDL BSSID: 00:25:00:ff:94:73                         │
│ - OUI: 00:17:f2 (Apple)                                  │
│ - Version, Type, Subtype                                 │
├──────────────────────────────────────────────────────────┤
│ TLV Payload (variable)                                   │
│ - Type (1 byte) + Length (2 bytes) + Value              │
└──────────────────────────────────────────────────────────┘
```

### Key TLV Types

| Type | Name | Purpose |
|------|------|---------|
| 4 | Sync Parameters | Clock sync, master address, AW timing |
| 5 | Election Parameters | Master metric, counter |
| 6 | Service Parameters | Service discovery |
| 18 | Channel Sequence | Channel hopping schedule |
| 20 | Sync Tree | Path to master (loop prevention) |
| 21 | Version | Protocol version |
| 24 | Election Parameters v2 | Extended election info |

### Data Frames

```
┌──────────────────────────────────────────────────────────┐
│ 802.11 Data Frame Header                                 │
├──────────────────────────────────────────────────────────┤
│ LLC Header (OUI: 00:17:f2)                              │
├──────────────────────────────────────────────────────────┤
│ AWDL Data Header                                         │
│ - Magic: 0x0304                                          │
│ - Sequence Number                                        │
│ - EtherType: 0x86dd (IPv6)                              │
├──────────────────────────────────────────────────────────┤
│ IPv6 Packet Payload                                      │
└──────────────────────────────────────────────────────────┘
```

---

## Implementation Paths

### Path A: Full AWDL Implementation (What Google Did)

**Pros:**
- Works with iPhones immediately (AirDrop "Everyone" mode)
- True peer-to-peer, no server needed
- High bandwidth (WiFi speeds)

**Cons:**
- Requires low-level WiFi driver access
- Need monitor mode + packet injection capability
- Complex synchronization logic
- Apple could break it in future iOS updates

**Requirements:**
- Linux: Monitor mode WiFi adapter + raw sockets
- Android: Root + custom kernel (or Pixel 10 which has it built-in)
- Windows: Difficult (need Npcap or similar)

**Existing Code:**
```bash
# OWL - C implementation for Linux
git clone https://github.com/seemoo-lab/owl
# Requires: libpcap, libev, libnl

# awdl-frame-parser - Rust frame parsing
# Can be used to build our own implementation
cargo add awdl-frame-parser
```

### Path B: WiFi Aware (NAN) - The Future

**EU DMA Mandate**: Apple MUST implement WiFi Aware 4.0 in iOS 19 (late 2025).

**Pros:**
- Open standard (WiFi Alliance)
- Android support since Android 8.0
- Will become the interoperable standard
- Apple legally required to support it

**Cons:**
- iOS doesn't support it YET
- Limited Android device support (not all chips)
- Less mature than AWDL

**Android API:**
```kotlin
// Check for WiFi Aware support
val wifiAwareManager = getSystemService(WifiAwareManager::class.java)
if (packageManager.hasSystemFeature(PackageManager.FEATURE_WIFI_AWARE)) {
    // Can use WiFi Aware
}
```

### Path C: Hybrid Approach (Recommended)

1. **For iOS NOW**: Use OpenDrop + OWL on a Linux bridge device (Raspberry Pi)
2. **For Android**: WiFi Aware where supported, fall back to WiFi Direct
3. **For Windows/macOS**: Native AWDL implementation (Google showed it's possible)
4. **Future**: Switch to WiFi Aware once iOS 19 ships

---

## AirDrop Protocol Details

### Discovery Flow

```
Sender                              Receiver
   │                                    │
   │─── BLE Advertisement ─────────────▶│
   │    (Apple ID hash, AirDrop flag)   │
   │                                    │
   │◀── AWDL Action Frames ─────────────│
   │    (PSF/MIF with service info)     │
   │                                    │
   │─── mDNS Query ────────────────────▶│
   │    _airdrop._tcp.local             │
   │                                    │
   │◀── mDNS Response ──────────────────│
   │    Device name, capabilities       │
   │                                    │
   │─── HTTPS POST /Discover ──────────▶│
   │    (Sender's contact hashes)       │
   │                                    │
   │◀── Discovery Response ─────────────│
   │    (Receiver info if contact match)│
```

### File Transfer Flow

```
Sender                              Receiver
   │                                    │
   │─── HTTPS POST /Ask ───────────────▶│
   │    {files: [...], sender: "..."}   │
   │                                    │
   │                              [User prompt]
   │                              [Accept/Reject]
   │                                    │
   │◀── /Ask Response ──────────────────│
   │    {accepted: true/false}          │
   │                                    │
   │─── HTTPS POST /Upload ────────────▶│
   │    (File data, chunked)            │
   │                                    │
   │◀── Upload Complete ────────────────│
```

### HTTP Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/Discover` | POST | Exchange contact hashes |
| `/Ask` | POST | Request permission to send |
| `/Upload` | POST | Transfer file data |

### TLS Details
- TLS 1.2 with client certificates
- Apple-signed certificates contain Apple ID validation record
- OpenDrop NOTE: Does NOT verify certificates (security gap, but makes implementation easier)

---

## Security Considerations

### What Google Did Right
1. **Rust implementation** - Memory safe, prevents buffer overflows
2. **External audit** - NetSPI penetration testing
3. **No server routing** - True P2P, no data logging

### Known AWDL Vulnerabilities
1. **Contact hash leakage** - Phone numbers recoverable in milliseconds
2. **No frame encryption** - All AWDL frames are plaintext
3. **DoS potential** - Can desync devices with malformed frames

### Our Approach Should:
1. Use Rust for frame parsing/generation (awdl-frame-parser exists!)
2. Implement proper TLS for application layer
3. Consider PrivateDrop's PSI protocol for privacy

---

## Proof of Concept Plan

### Phase 1: Linux PoC (2-3 weeks)
1. Set up OWL on Raspberry Pi / Linux laptop
2. Get OpenDrop working for AirDrop receive
3. Test receiving files FROM iPhone

### Phase 2: Rust Rewrite (4-6 weeks)
1. Use awdl-frame-parser crate as foundation
2. Implement AWDL state machine in Rust
3. Create Linux daemon with virtual network interface
4. Test bidirectional transfers

### Phase 3: Android Integration (6-8 weeks)
1. Research Android WiFi HAL for raw frame access
2. Implement AWDL as native library
3. JNI bridge to Kotlin app
4. Test on rooted device first

### Phase 4: Production
1. WiFi Aware implementation for non-rooted Android
2. Windows driver research
3. Wait for iOS 19 WiFi Aware support

---

## Resources

### Code Repositories
- [OWL](https://github.com/seemoo-lab/owl) - Linux AWDL implementation (C)
- [OpenDrop](https://github.com/seemoo-lab/opendrop) - AirDrop protocol (Python)
- [awdl-frame-parser](https://github.com/Frostie314159/awdl-frame-parser) - Frame parser (Rust)
- [PrivateDrop](https://github.com/seemoo-lab/privatedrop) - Secure variant
- [NearDrop](https://github.com/grishka/NearDrop) - Quick Share for macOS

### Research Papers
- [One Billion Apples' Secret Sauce](https://arxiv.org/pdf/1808.03156) - AWDL reverse engineering (MobiCom '18)
- [A Billion Open Interfaces](https://www.usenix.org/system/files/sec19-stute.pdf) - Security analysis (USENIX '19)
- [PrivateDrop Paper](https://www.usenix.org/system/files/sec21-heinrich.pdf) - Privacy vulnerabilities (USENIX '21)

### Official Documentation
- [Android WiFi Aware](https://developer.android.com/develop/connectivity/wifi/wifi-aware)
- [Apple MultipeerConnectivity](https://developer.apple.com/documentation/multipeerconnectivity)
- [Ditto Blog on AWDL/DMA](https://www.ditto.com/blog/cross-platform-p2p-wi-fi-how-the-eu-killed-awdl)

### Tools
- [Wireshark AWDL Dissector](https://github.com/nicklashansen/awdl-dissector) - Packet analysis
- [WiFi Aware Checker](https://github.com/getditto/wifi-aware-checker) - Android support check

---

## Timeline Considerations

### Now (Early 2026)
- AWDL works, but requires low-level access
- Google has proven it's possible (Pixel 10)
- EU DMA mandates WiFi Aware in iOS 19

### iOS 19 (Late 2026)
- Apple MUST implement WiFi Aware
- Cross-platform becomes much easier
- Our AWDL work still valuable for older iOS devices

### Recommendation
Start with AWDL implementation now (proves the concept, works with existing iPhones), but architect for WiFi Aware migration when iOS 19 ships.

---

## Next Steps

1. **Immediate**: Set up Linux dev environment with OWL + OpenDrop
2. **This Week**: Test receiving AirDrop from iPhone on Linux
3. **Next Sprint**: Start Rust AWDL implementation using awdl-frame-parser
4. **Parallel**: Research Android raw WiFi access options
