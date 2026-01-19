/**
 * Drop - Lightweight File Sharing
 * Sender Web App
 */

// Drop BLE Service and Characteristic UUIDs
const DROP_SERVICE_UUID = '0000fe50-0000-1000-8000-00805f9b34fb';
const CHAR_DEVICE_INFO_UUID = '0000fe51-0000-1000-8000-00805f9b34fb';
const CHAR_TRANSFER_REQUEST_UUID = '0000fe52-0000-1000-8000-00805f9b34fb';
const CHAR_TRANSFER_RESPONSE_UUID = '0000fe53-0000-1000-8000-00805f9b34fb';
const CHAR_TRANSFER_STATUS_UUID = '0000fe54-0000-1000-8000-00805f9b34fb';

// State
const state = {
    files: [],
    devices: [],
    selectedDevice: null,
    bleDevice: null,
    bleServer: null,
    bleService: null,
    transferInProgress: false,
    abortController: null
};

// DOM Elements
const elements = {
    // Steps
    stepFiles: document.getElementById('step-files'),
    stepDevices: document.getElementById('step-devices'),
    stepTransfer: document.getElementById('step-transfer'),
    stepUnsupported: document.getElementById('step-unsupported'),

    // File selection
    dropZone: document.getElementById('dropZone'),
    fileInput: document.getElementById('fileInput'),
    browseBtn: document.getElementById('browseBtn'),
    selectedFiles: document.getElementById('selectedFiles'),
    fileListItems: document.getElementById('fileListItems'),
    fileSummary: document.getElementById('fileSummary'),
    clearFiles: document.getElementById('clearFiles'),
    nextBtn: document.getElementById('nextBtn'),

    // Device selection
    backToFiles: document.getElementById('backToFiles'),
    scanStatus: document.getElementById('scanStatus'),
    deviceList: document.getElementById('deviceList'),
    deviceListItems: document.getElementById('deviceListItems'),
    noDevices: document.getElementById('noDevices'),
    rescanBtn: document.getElementById('rescanBtn'),

    // Transfer
    transferTitle: document.getElementById('transferTitle'),
    transferSubtitle: document.getElementById('transferSubtitle'),
    receiverName: document.getElementById('receiverName'),
    progressContainer: document.getElementById('progressContainer'),
    progressFill: document.getElementById('progressFill'),
    progressPercent: document.getElementById('progressPercent'),
    progressSpeed: document.getElementById('progressSpeed'),
    transferComplete: document.getElementById('transferComplete'),
    transferError: document.getElementById('transferError'),
    errorMessage: document.getElementById('errorMessage'),
    cancelTransfer: document.getElementById('cancelTransfer'),
    retryBtn: document.getElementById('retryBtn')
};

// Initialize
function init() {
    // Check for Web Bluetooth support
    if (!navigator.bluetooth) {
        showStep('unsupported');
        return;
    }

    setupEventListeners();
    showStep('files');
}

function setupEventListeners() {
    // File selection
    elements.dropZone.addEventListener('click', () => elements.fileInput.click());
    elements.browseBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        elements.fileInput.click();
    });
    elements.fileInput.addEventListener('change', handleFileSelect);
    elements.clearFiles.addEventListener('click', clearFiles);
    elements.nextBtn.addEventListener('click', goToDeviceScan);

    // Drag and drop
    elements.dropZone.addEventListener('dragover', handleDragOver);
    elements.dropZone.addEventListener('dragleave', handleDragLeave);
    elements.dropZone.addEventListener('drop', handleDrop);

    // Device selection
    elements.backToFiles.addEventListener('click', () => showStep('files'));
    elements.rescanBtn.addEventListener('click', scanForDevices);

    // Transfer
    elements.cancelTransfer.addEventListener('click', cancelTransfer);
    elements.retryBtn.addEventListener('click', retryTransfer);
}

// Step Navigation
function showStep(step) {
    elements.stepFiles.classList.remove('active');
    elements.stepDevices.classList.remove('active');
    elements.stepTransfer.classList.remove('active');
    elements.stepUnsupported.classList.remove('active');

    switch (step) {
        case 'files':
            elements.stepFiles.classList.add('active');
            break;
        case 'devices':
            elements.stepDevices.classList.add('active');
            break;
        case 'transfer':
            elements.stepTransfer.classList.add('active');
            break;
        case 'unsupported':
            elements.stepUnsupported.classList.add('active');
            break;
    }
}

// File Handling
function handleFileSelect(e) {
    const files = Array.from(e.target.files);
    addFiles(files);
}

function handleDragOver(e) {
    e.preventDefault();
    elements.dropZone.classList.add('dragover');
}

function handleDragLeave(e) {
    e.preventDefault();
    elements.dropZone.classList.remove('dragover');
}

function handleDrop(e) {
    e.preventDefault();
    elements.dropZone.classList.remove('dragover');
    const files = Array.from(e.dataTransfer.files);
    addFiles(files);
}

function addFiles(files) {
    state.files = [...state.files, ...files];
    updateFileList();
}

function clearFiles() {
    state.files = [];
    elements.fileInput.value = '';
    updateFileList();
}

function updateFileList() {
    if (state.files.length === 0) {
        elements.selectedFiles.classList.add('hidden');
        elements.nextBtn.classList.add('hidden');
        elements.nextBtn.disabled = true;
        return;
    }

    elements.selectedFiles.classList.remove('hidden');
    elements.nextBtn.classList.remove('hidden');
    elements.nextBtn.disabled = false;

    // Render file list
    elements.fileListItems.innerHTML = state.files.map((file, index) => `
        <li>
            <div class="file-info">
                <div class="file-icon">${getFileIcon(file.type)}</div>
                <span class="file-name">${escapeHtml(file.name)}</span>
            </div>
            <span class="file-size">${formatSize(file.size)}</span>
        </li>
    `).join('');

    // Update summary
    const totalSize = state.files.reduce((sum, f) => sum + f.size, 0);
    elements.fileSummary.textContent = `${state.files.length} file${state.files.length > 1 ? 's' : ''} · ${formatSize(totalSize)}`;
}

function getFileIcon(mimeType) {
    if (mimeType.startsWith('image/')) return '🖼';
    if (mimeType.startsWith('video/')) return '🎬';
    if (mimeType.startsWith('audio/')) return '🎵';
    if (mimeType.includes('pdf')) return '📄';
    if (mimeType.includes('zip') || mimeType.includes('rar')) return '📦';
    return '📁';
}

// Device Scanning
async function goToDeviceScan() {
    showStep('devices');
    await scanForDevices();
}

async function scanForDevices() {
    // Reset UI
    elements.scanStatus.classList.remove('hidden');
    elements.deviceList.classList.add('hidden');
    elements.noDevices.classList.add('hidden');
    state.devices = [];

    try {
        // Request BLE device with Drop service
        // Note: Web Bluetooth requires user gesture and shows a picker
        const device = await navigator.bluetooth.requestDevice({
            filters: [{ services: [DROP_SERVICE_UUID] }],
            optionalServices: [DROP_SERVICE_UUID]
        });

        // User selected a device
        state.devices = [device];
        state.bleDevice = device;

        elements.scanStatus.classList.add('hidden');

        if (state.devices.length > 0) {
            renderDeviceList();
            elements.deviceList.classList.remove('hidden');

            // Auto-select if only one device
            if (state.devices.length === 1) {
                selectDevice(device);
            }
        } else {
            elements.noDevices.classList.remove('hidden');
        }

    } catch (error) {
        console.error('Scan error:', error);
        elements.scanStatus.classList.add('hidden');

        if (error.name === 'NotFoundError') {
            // User cancelled the picker or no devices found
            elements.noDevices.classList.remove('hidden');
        } else {
            showError('Bluetooth scan failed: ' + error.message);
        }
    }
}

function renderDeviceList() {
    elements.deviceListItems.innerHTML = state.devices.map((device, index) => `
        <li data-index="${index}" onclick="selectDevice(state.devices[${index}])">
            <div class="device-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <rect x="5" y="2" width="14" height="20" rx="2" ry="2"/>
                    <line x1="12" y1="18" x2="12.01" y2="18"/>
                </svg>
            </div>
            <div class="device-info">
                <div class="device-name">${escapeHtml(device.name || 'Unknown Device')}</div>
                <div class="device-type">Drop Receiver</div>
            </div>
            <div class="device-signal strong">
                <span></span><span></span><span></span><span></span>
            </div>
        </li>
    `).join('');
}

async function selectDevice(device) {
    state.selectedDevice = device;
    state.bleDevice = device;
    await initiateTransfer();
}

// Transfer
async function initiateTransfer() {
    showStep('transfer');
    resetTransferUI();

    elements.receiverName.textContent = state.selectedDevice.name || 'device';
    elements.transferTitle.textContent = 'Connecting...';
    elements.transferSubtitle.textContent = 'Establishing BLE connection';

    state.abortController = new AbortController();

    try {
        // Connect to GATT server
        console.log('Connecting to GATT server...');
        state.bleServer = await state.bleDevice.gatt.connect();

        // Get Drop service
        console.log('Getting Drop service...');
        state.bleService = await state.bleServer.getPrimaryService(DROP_SERVICE_UUID);

        // Get characteristics
        const responseChar = await state.bleService.getCharacteristic(CHAR_TRANSFER_RESPONSE_UUID);
        const requestChar = await state.bleService.getCharacteristic(CHAR_TRANSFER_REQUEST_UUID);

        // Subscribe to response notifications
        await responseChar.startNotifications();

        const responsePromise = new Promise((resolve, reject) => {
            const timeout = setTimeout(() => {
                reject(new Error('Transfer request timed out'));
            }, 60000); // 60 second timeout

            responseChar.addEventListener('characteristicvaluechanged', (event) => {
                clearTimeout(timeout);
                const value = new TextDecoder().decode(event.target.value);
                try {
                    resolve(JSON.parse(value));
                } catch {
                    resolve({ raw: value });
                }
            }, { once: true });
        });

        // Build transfer request
        const totalSize = state.files.reduce((sum, f) => sum + f.size, 0);
        const transferRequest = {
            type: 'transfer_request',
            sender: 'Drop Web',
            files: state.files.map(f => ({
                name: f.name,
                size: f.size,
                type: f.type || 'application/octet-stream'
            })),
            totalSize: totalSize
        };

        // Send transfer request
        elements.transferTitle.textContent = 'Sending request...';
        elements.transferSubtitle.innerHTML = `Waiting for <span id="receiverName">${escapeHtml(state.selectedDevice.name || 'device')}</span> to accept`;

        console.log('Sending transfer request:', transferRequest);
        const requestData = new TextEncoder().encode(JSON.stringify(transferRequest));
        await requestChar.writeValue(requestData);

        // Wait for response
        const response = await responsePromise;
        console.log('Received response:', response);

        if (response.accepted) {
            // Start file transfer over HTTP
            await transferFiles(response.ip, response.port);
        } else {
            throw new Error(response.reason || 'Transfer rejected');
        }

    } catch (error) {
        console.error('Transfer error:', error);

        if (error.name !== 'AbortError') {
            showTransferError(error.message);
        }
    }
}

async function transferFiles(ip, port) {
    elements.transferTitle.textContent = 'Transferring...';
    elements.transferSubtitle.textContent = `Sending to ${state.selectedDevice.name || 'device'}`;
    elements.progressContainer.classList.remove('hidden');

    const totalSize = state.files.reduce((sum, f) => sum + f.size, 0);
    let transferred = 0;
    const startTime = Date.now();

    try {
        // Get status characteristic for progress updates
        const statusChar = await state.bleService.getCharacteristic(CHAR_TRANSFER_STATUS_UUID);

        for (let i = 0; i < state.files.length; i++) {
            const file = state.files[i];

            // Create form data
            const formData = new FormData();
            formData.append('file', file);
            formData.append('index', i.toString());
            formData.append('total', state.files.length.toString());

            // Upload file
            const uploadUrl = `http://${ip}:${port}/upload`;
            console.log(`Uploading ${file.name} to ${uploadUrl}`);

            const response = await fetch(uploadUrl, {
                method: 'POST',
                body: formData,
                signal: state.abortController.signal
            });

            if (!response.ok) {
                throw new Error(`Upload failed: ${response.status}`);
            }

            transferred += file.size;

            // Update progress
            const percent = Math.round((transferred / totalSize) * 100);
            const elapsed = (Date.now() - startTime) / 1000;
            const speed = transferred / elapsed;

            elements.progressFill.style.width = `${percent}%`;
            elements.progressPercent.textContent = percent;
            elements.progressSpeed.textContent = formatSpeed(speed);
        }

        // Send completion status via BLE
        const completeData = new TextEncoder().encode(JSON.stringify({ type: 'complete' }));
        await statusChar.writeValue(completeData);

        // Show success
        showTransferComplete();

    } catch (error) {
        if (error.name !== 'AbortError') {
            throw error;
        }
    }
}

function showTransferComplete() {
    elements.transferTitle.textContent = 'Done!';
    elements.transferSubtitle.textContent = '';
    elements.progressContainer.classList.add('hidden');
    elements.transferComplete.classList.remove('hidden');
    elements.cancelTransfer.textContent = 'Send More';
    elements.cancelTransfer.onclick = () => {
        clearFiles();
        showStep('files');
    };
}

function showTransferError(message) {
    elements.transferTitle.textContent = 'Transfer Failed';
    elements.transferSubtitle.textContent = '';
    elements.progressContainer.classList.add('hidden');
    elements.transferError.classList.remove('hidden');
    elements.errorMessage.textContent = message;
}

function resetTransferUI() {
    elements.progressContainer.classList.add('hidden');
    elements.progressFill.style.width = '0%';
    elements.progressPercent.textContent = '0';
    elements.progressSpeed.textContent = '--';
    elements.transferComplete.classList.add('hidden');
    elements.transferError.classList.add('hidden');
    elements.cancelTransfer.textContent = 'Cancel';
    elements.cancelTransfer.onclick = cancelTransfer;
}

function cancelTransfer() {
    if (state.abortController) {
        state.abortController.abort();
    }

    if (state.bleServer && state.bleServer.connected) {
        state.bleServer.disconnect();
    }

    state.transferInProgress = false;
    showStep('files');
}

function retryTransfer() {
    if (state.selectedDevice) {
        initiateTransfer();
    } else {
        showStep('devices');
    }
}

function showError(message) {
    // Simple error display - could be enhanced
    alert(message);
}

// Utilities
function formatSize(bytes) {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

function formatSpeed(bytesPerSecond) {
    if (bytesPerSecond === 0) return '0 B/s';
    const k = 1024;
    const sizes = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
    const i = Math.floor(Math.log(bytesPerSecond) / Math.log(k));
    return parseFloat((bytesPerSecond / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// Make selectDevice available globally for onclick handlers
window.selectDevice = selectDevice;
window.state = state;

// Start the app
init();
