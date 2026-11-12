# Packaging, Distribution & Auto-Update Architecture

CraftBrowsing is distributed across **Windows, macOS, and Linux** using native packaging standards, hardened code-signing pipelines, and bandwidth-efficient, cryptographically verified auto-update mechanisms.

---

## 1. Platform Matrix & Distribution Targets

| Platform | Target Architecture | Primary Package Format | Auto-Update Mechanism | Signature & Verification |
| :--- | :--- | :--- | :--- | :--- |
| **Windows** | `x86_64-pc-windows-msvc`<br>`aarch64-pc-windows-msvc` | `.msix` / `.exe` (NSIS silent installer) | Background differential update service | Microsoft Authenticode & Ed25519 payload signature |
| **macOS** | `x86_64-apple-darwin`<br>`aarch64-apple-darwin` (Universal) | `.dmg` / `.app` bundle | Sparkle-compatible / native Rust update protocol | Apple Developer ID Application + Hardened Runtime & Notarization |
| **Linux** | `x86_64-unknown-linux-gnu`<br>`aarch64-unknown-linux-gnu` | **AppImage** (standalone portable bundle) | **`zsync`** block-level differential updates | Minisign / Ed25519 embedded digest verification |

---

## 2. Linux AppImage with `zsync` Architecture

On Linux, CraftBrowsing provides first-class portable releases via **AppImage** combined with **`zsync`** to eliminate redundant multi-hundred-megabyte re-downloads when updates are published.

### 2.1. How `zsync` Differential Updates Work

```text
┌─────────────────────────┐               ┌──────────────────────────────────────────┐
│  CraftBrowsing Client   │               │            GitHub Releases / CDN         │
│  (Existing v0.1.0 .App) │               │                                          │
└────────────┬────────────┘               └────────────────────┬─────────────────────┘
             │                                                 │
             │ 1. Fetch small .zsync control file (~100 KB)    │
             ├────────────────────────────────────────────────►│ CraftBrowsing-latest.zsync
             │                                                 │
             │ 2. Compute local block checksums against control│
             │    file and identify missing byte ranges        │
             │                                                 │
             │ 3. HTTP Range requests for changed blocks only  │
             ├────────────────────────────────────────────────►│ CraftBrowsing-v0.2.0.AppImage
             │                                                 │
             │ 4. Reconstruct v0.2.0 AppImage locally & verify │
             │    Ed25519 signature before atomic swap         │
             ▼                                                 ▼
```

1. **`.zsync` Metafile Generation:** During CI/CD release builds, `zsyncmake` analyzes the compressed AppImage binary and generates a companion `CraftBrowsing-x86_64.AppImage.zsync` file containing rolling checksums (MD4/Rabin fingerprint) for each block.
2. **Byte-Level Delta Downloads:** When CraftBrowsing checks for updates, it only downloads the `.zsync` index file (~100 KB). The client compares its local AppImage against the remote block hashes, determining which blocks are identical.
3. **HTTP Range Requests:** The update client requests *only the modified byte ranges* from the remote server, typically reducing download payloads from ~120 MB down to **5–15 MB** per release.
4. **In-Place Atomic Replacement:** The new AppImage is re-assembled in a temporary directory, validated against the project's embedded Ed25519 public key, and atomically replaces or points the desktop launcher to the new AppImage.
5. **AppImageUpdate Integration:** Fully interoperable with standard desktop AppImage updaters (`appimageupdatetool`) via embedded `X-AppImage-UpdateInformation` headers:
   ```desktop
   X-AppImage-UpdateInformation=zsync|https://github.com/cmspeedau/CraftBrowsing/releases/latest/download/CraftBrowsing-x86_64.AppImage.zsync
   ```

---

## 3. Windows Auto-Update Pipeline

On Windows, CraftBrowsing provides a smooth background update experience without intrusive UAC prompts during routine browser patching:

1. **Installer Targets:**
   - **Per-User (Default, No UAC):** Installs into `%LOCALAPPDATA%\CraftBrowsing\`. Background updates execute without elevated administrator permissions.
   - **Machine-Wide (Enterprise):** Installs into `Program Files\CraftBrowsing\` with an optional background Windows service for managed corporate deployments.
2. **Update Check & Delta Application:**
   - The browser polls the release manifest endpoint using privacy-preserving, identifier-free requests (no hardware GUIDs, no tracking parameters).
   - Downloads signed binary diff patches or full fallbacks over TLS.
   - Stage-and-swap mechanism: Updates are extracted into a `new_version/` staging directory. When the browser restarts, a lightweight helper binary (`craft-update-helper.exe`) atomically swaps the executable directory.
3. **Code Signing & Integrity:**
   - All executables and DLLs are signed with a trusted Microsoft Authenticode certificate.
   - Staged update archives must match the signed SHA256 digest listed in the release manifest.

---

## 4. macOS Auto-Update Pipeline

On macOS, CraftBrowsing supports native Apple Silicon and Intel hardware with strict compliance with Apple's Hardened Runtime and Gatekeeper standards:

1. **Universal Binary Packaging:** CI builds compile both `x86_64-apple-darwin` and `aarch64-apple-darwin`, binding them via `lipo` into a single universal `.app` bundle packaged inside a signed `.dmg`.
2. **Hardened Runtime & Notarization:**
   - Binaries are signed with an Apple Developer ID Application certificate with Hardened Runtime enabled.
   - Every release candidate is submitted to Apple's automated notarization service (`xcrun notarytool`) and stapled prior to publishing.
3. **In-App Auto-Update:**
   - Uses an embedded update client adhering to Ed25519 signature verification.
   - Downloads and unpacks update archives into a sandboxed staging area, validates codesigning identity via `SecCodeCheckValidity`, and replaces the app bundle seamlessly upon user restart.

---

## 5. Security & Privacy Guarantees for Updates

1. **User Control & Killswitch:**
   - Users can choose between **Automatic**, **Notify Only**, or **Completely Disabled** in `craft://settings/updates`.
   - Never forces surprise restarts or unexpected bandwidth consumption.
2. **Zero-Telemetry Update Pings:**
   - Update checks send strictly generic HTTP `GET` requests for the static release manifest. No cookies, no IP logging, no device fingerprints, and no persistent client identifiers are transmitted.
3. **Cryptographic Verification (Ed25519):**
   - Every release manifest and binary payload is signed with the CraftBrowsing offline Ed25519 root release key.
   - The client verifies signatures before any downloaded bytes are executed or unpacked. If a signature fails, the payload is purged immediately.
