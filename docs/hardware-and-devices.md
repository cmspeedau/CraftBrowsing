# Hardware & Device Access Specification (WebSerial / WebUSB / WebHID)

CraftBrowsing supports device-level hardware communication APIs (WebSerial, WebUSB, and WebHID) enabling in-browser firmware flashing, embedded microcontroller debugging (ESP32, Arduino, Raspberry Pi Pico), hardware wallets, and peripheral configuration (mechanical keyboard flashing via QMK/VIA).

Because raw hardware access carries severe security and tracking risks, CraftBrowsing implements a **Zero-Trust, Permission-First architecture**.

---

## 1. Threat Model & Security Posture

Raw bus access allows malicious sites to read firmware, dump memory, or flash rogue bootloaders to connected hardware. CraftBrowsing addresses this with five hard barriers:

1. **Disabled by Default:**
   All hardware device APIs (`navigator.serial`, `navigator.usb`, `navigator.hid`) are disabled out of the box. Users must explicitly toggle hardware support on in `craft://settings/hardware`.
2. **Explicit User Activation Required:**
   No web origin can passively enumerate, scan, or query connected USB or COM ports. Device discovery calls (`requestPort()`, `requestDevice()`) **must** be triggered directly by an explicit user gesture (such as an interactive button click).
3. **Native OS-Level Device Picker:**
   The browser displays a secure, out-of-process device selection dialog showing:
   - Vendor ID (VID) and Product ID (PID)
   - Manufacturer name and product description string
   - Active serial port path (e.g. `/dev/ttyUSB0`, `COM3`)
   - Prominent security advisory warning of raw read/write capabilities
4. **Container Isolation & Ephemeral Grants:**
   Permissions are strictly bound to the active container session and originating domain tuple (`origin`, `container_id`). Users can choose between **"Allow Once (until tab closes or unplugged)"** and persistent remembered pairing.
5. **Visual Bus Indicator & Instant Killswitch:**
   Whenever a hardware stream is active, the address bar displays an active hardware icon (cable/chip badge). Clicking the badge shows real-time transfer counters (bytes read/written) and a one-click **"Force Disconnect"** button that immediately drops the native file descriptor / port handle.

---

## 2. Architecture & Rust Implementation (`craft-hardware`)

```text
┌─────────────────────────────────────────────────────────────┐
│                 Web Page (JavaScript Engine)                │
│       navigator.serial / navigator.usb / navigator.hid       │
└──────────────────────────────┬──────────────────────────────┘
                               │ IPC / C Bindings
┌──────────────────────────────▼──────────────────────────────┐
│                    craft-engine (Servo DOM)                 │
│         Validates User Gesture & Origin Context             │
└──────────────────────────────┬──────────────────────────────┘
                               │ Permission Request
┌──────────────────────────────▼──────────────────────────────┐
│                    craft-app (Chrome UI)                    │
│     Prompts User with Native Modal Device Selector          │
└──────────────────────────────┬──────────────────────────────┘
                               │ Approved Device Handle
┌──────────────────────────────▼──────────────────────────────┐
│                   craft-hardware Crate                      │
├──────────────────────┬──────────────────────┬───────────────┤
│    Serial Engine     │      USB Engine      │  HID Engine   │
│   (`tokio-serial`)   │       (`rusb`)       │  (`hidapi`)   │
└──────────────────────┴──────────────────────┴───────────────┘
```

### Supported APIs

| API | Backend Rust Crate | Target Use Cases |
| :--- | :--- | :--- |
| **WebSerial** | `serialport` / `tokio-serial` | ESP32/ESP8266 flashing (ESP Web Flasher), Arduino, MicroPython REPL, 3D printers |
| **WebUSB** | `rusb` (libusb bindings) | WebDFU firmware flashing, Android Fastboot, Flipper Zero web tools |
| **WebHID** | `hidapi` | QMK / VIA keyboard configurators, YubiKey / FIDO hardware management |

---

## 3. UI and Management: `craft://settings/hardware`

The internal configuration interface provides:
- **Master Hardware Switch:** Toggle all hardware APIs on or off globally.
- **Connected Device Inspector:** Real-time view of currently authorized devices and open ports.
- **Revocation Manager:** One-click revocation of all remembered device permissions per container or domain.
- **Baud Rate Limiting & Safety Flags:** Optional warnings when non-standard baud rates or DTR/RTS signal line manipulation is initiated.
