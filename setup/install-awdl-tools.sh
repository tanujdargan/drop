#!/bin/bash
#
# Drop Phase 1: AWDL Tools Installer
# Installs OWL (AWDL daemon) and OpenDrop (AirDrop protocol)
#
# Usage: sudo ./install-awdl-tools.sh
#

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    log_error "Please run as root (sudo ./install-awdl-tools.sh)"
    exit 1
fi

# Detect distro
if [ -f /etc/os-release ]; then
    . /etc/os-release
    DISTRO=$ID
else
    log_error "Cannot detect distribution"
    exit 1
fi

log_info "Detected distribution: $DISTRO"

# Install dependencies based on distro
install_deps() {
    case $DISTRO in
        ubuntu|debian|kali)
            log_info "Installing dependencies via apt..."
            apt update
            apt install -y \
                build-essential \
                git \
                cmake \
                libpcap-dev \
                libev-dev \
                libnl-3-dev \
                libnl-genl-3-dev \
                python3 \
                python3-pip \
                python3-dev \
                libssl-dev \
                libffi-dev \
                aircrack-ng \
                iw \
                wireless-tools \
                net-tools
            ;;
        fedora)
            log_info "Installing dependencies via dnf..."
            dnf install -y \
                @development-tools \
                git \
                cmake \
                libpcap-devel \
                libev-devel \
                libnl3-devel \
                python3 \
                python3-pip \
                python3-devel \
                openssl-devel \
                libffi-devel \
                aircrack-ng \
                iw \
                wireless-tools \
                net-tools
            ;;
        arch|manjaro)
            log_info "Installing dependencies via pacman..."
            pacman -Sy --noconfirm \
                base-devel \
                git \
                cmake \
                libpcap \
                libev \
                libnl \
                python \
                python-pip \
                openssl \
                libffi \
                aircrack-ng \
                iw \
                wireless_tools \
                net-tools
            ;;
        *)
            log_error "Unsupported distribution: $DISTRO"
            log_warn "Please install dependencies manually:"
            echo "  - build-essential/base-devel"
            echo "  - git, cmake"
            echo "  - libpcap-dev, libev-dev, libnl-3-dev, libnl-genl-3-dev"
            echo "  - python3, python3-pip"
            echo "  - aircrack-ng, iw, wireless-tools"
            exit 1
            ;;
    esac
}

# Install OWL
install_owl() {
    log_info "Installing OWL (AWDL daemon)..."

    INSTALL_DIR="/opt/drop"
    mkdir -p $INSTALL_DIR
    cd $INSTALL_DIR

    if [ -d "owl" ]; then
        log_warn "OWL directory exists, updating..."
        cd owl
        git pull
    else
        git clone https://github.com/seemoo-lab/owl.git
        cd owl
    fi

    # Build
    rm -rf build
    mkdir build && cd build
    cmake ..
    make -j$(nproc)

    # Create symlink
    ln -sf $INSTALL_DIR/owl/build/owl /usr/local/bin/owl

    log_info "OWL installed successfully!"
    log_info "Binary: /usr/local/bin/owl"
}

# Install OpenDrop
install_opendrop() {
    log_info "Installing OpenDrop (AirDrop protocol)..."

    INSTALL_DIR="/opt/drop"
    cd $INSTALL_DIR

    if [ -d "opendrop" ]; then
        log_warn "OpenDrop directory exists, updating..."
        cd opendrop
        git pull
    else
        git clone https://github.com/seemoo-lab/opendrop.git
        cd opendrop
    fi

    # Install via pip
    pip3 install --break-system-packages . 2>/dev/null || pip3 install .

    log_info "OpenDrop installed successfully!"
    log_info "Command: opendrop"
}

# Install RTL8812AU driver (for Alfa adapters)
install_rtl8812au_driver() {
    log_info "Installing RTL8812AU driver (for Alfa AWUS036ACH)..."

    INSTALL_DIR="/opt/drop"
    cd $INSTALL_DIR

    if [ -d "rtl8812au" ]; then
        log_warn "rtl8812au directory exists, updating..."
        cd rtl8812au
        git pull
    else
        git clone https://github.com/aircrack-ng/rtl8812au.git
        cd rtl8812au
    fi

    # Build and install
    make -j$(nproc)
    make install

    log_info "RTL8812AU driver installed!"
    log_info "Load with: modprobe 88XXau"
}

# Create helper scripts
create_helper_scripts() {
    log_info "Creating helper scripts..."

    # Start script
    cat > /usr/local/bin/drop-start << 'EOF'
#!/bin/bash
# Start Drop AWDL receiver

IFACE="${1:-wlan1}"

if [ "$EUID" -ne 0 ]; then
    echo "Please run as root"
    exit 1
fi

echo "[*] Starting Drop AWDL receiver on $IFACE"

# Enable monitor mode
echo "[*] Enabling monitor mode..."
ip link set $IFACE down
iw $IFACE set monitor none
ip link set $IFACE up

# Start OWL in background
echo "[*] Starting OWL daemon..."
owl -i $IFACE &
OWL_PID=$!

sleep 2

# Check if awdl0 interface exists
if ! ip link show awdl0 &>/dev/null; then
    echo "[!] Failed to create awdl0 interface"
    kill $OWL_PID 2>/dev/null
    exit 1
fi

echo "[*] AWDL interface ready: awdl0"
echo "[*] Starting OpenDrop receiver..."
echo ""
echo "======================================"
echo "  Drop is ready to receive AirDrop!"
echo "======================================"
echo ""
echo "On your iPhone:"
echo "  1. Open AirDrop settings"
echo "  2. Set to 'Everyone for 10 Minutes'"
echo "  3. Select a file and tap Share → AirDrop"
echo "  4. Select this device"
echo ""
echo "Press Ctrl+C to stop"
echo ""

# Start OpenDrop
opendrop receive

# Cleanup on exit
kill $OWL_PID 2>/dev/null
ip link set $IFACE down
iw $IFACE set type managed
ip link set $IFACE up
echo "[*] Stopped"
EOF
    chmod +x /usr/local/bin/drop-start

    # Stop script
    cat > /usr/local/bin/drop-stop << 'EOF'
#!/bin/bash
# Stop Drop AWDL receiver

killall owl 2>/dev/null
killall opendrop 2>/dev/null

# Reset interface if specified
IFACE="${1:-wlan1}"
ip link set $IFACE down 2>/dev/null
iw $IFACE set type managed 2>/dev/null
ip link set $IFACE up 2>/dev/null

echo "[*] Drop stopped"
EOF
    chmod +x /usr/local/bin/drop-stop

    # Find devices script
    cat > /usr/local/bin/drop-find << 'EOF'
#!/bin/bash
# Find nearby AirDrop devices

echo "[*] Searching for AirDrop devices..."
opendrop find
EOF
    chmod +x /usr/local/bin/drop-find

    log_info "Helper scripts created:"
    log_info "  - drop-start <interface>  : Start receiving"
    log_info "  - drop-stop <interface>   : Stop receiving"
    log_info "  - drop-find               : Find nearby devices"
}

# Check WiFi adapter
check_wifi_adapter() {
    log_info "Checking for WiFi adapters..."

    echo ""
    echo "Available wireless interfaces:"
    iw dev | grep -E "Interface|type" | sed 's/^/  /'
    echo ""

    # Check for monitor mode support
    log_warn "Note: Your adapter MUST support monitor mode and packet injection"
    log_warn "Intel adapters (AX211, etc.) do NOT work - you need a USB adapter"
    echo ""
    echo "Recommended adapters:"
    echo "  - Alfa AWUS036ACH (RTL8812AU) - Best choice, dual-band"
    echo "  - Alfa AWUS036NHA (AR9271)    - Budget, 2.4GHz only"
    echo "  - Panda PAU09 (RT5572)        - Budget, dual-band"
    echo ""
}

# Main installation
main() {
    echo "========================================"
    echo "  Drop Phase 1: AWDL Tools Installer"
    echo "========================================"
    echo ""

    install_deps
    install_owl
    install_opendrop
    create_helper_scripts

    echo ""
    log_info "Installation complete!"
    echo ""

    check_wifi_adapter

    echo "Next steps:"
    echo "  1. Get a compatible USB WiFi adapter (see above)"
    echo "  2. Plug it in and find interface name: iw dev"
    echo "  3. Run: sudo drop-start <interface>"
    echo "  4. On iPhone, AirDrop a file to this device"
    echo ""
    echo "Optional: Install RTL8812AU driver for Alfa adapters:"
    echo "  sudo $0 --driver"
}

# Handle arguments
case "${1:-}" in
    --driver)
        install_rtl8812au_driver
        ;;
    --help|-h)
        echo "Usage: sudo $0 [--driver]"
        echo ""
        echo "Options:"
        echo "  --driver    Install RTL8812AU driver for Alfa adapters"
        echo "  --help      Show this help"
        ;;
    *)
        main
        ;;
esac
