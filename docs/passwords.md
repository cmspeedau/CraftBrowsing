# Password Manager Integration

CraftBrowsing delegates credentials management to dedicated tools such as **Bitwarden**, **1Password**, and **KeePassXC**, avoiding custom cryptographic secrets management in browser code.

---

## Integration Strategies

### 1. WebExtensions + Native Messaging (Primary)

The standard Bitwarden and 1Password browser extensions communicate with desktop clients via **Native Messaging Hosts**:

- **Content Script Injection:** Injects form field listeners into web pages to detect username and password inputs.
- **Native Host IPC:** The browser reads the native messaging manifest (e.g. `~/.mozilla/native-messaging-hosts/com.8bit.bitwarden.json`), spawns the desktop client binary, and proxies 32-bit length-prefixed JSON packets over `stdin`/`stdout`.
- **Biometric Unlock:** Enables system-level biometric unlocking (Touch ID / Windows Hello) without browser modifications.

### 2. KeePassXC Unix Socket (Alternative)

- Communicates with `keepassxc-proxy` over a local Unix domain socket.
- Uses encrypted JSON-RPC (libsodium/NaCl) for querying credentials matching the current tab's domain.

### 3. CLI Bridge (`bw` / `pass`)

- For lightweight setups without GUI background clients, CraftBrowsing can interface directly with `bw list items --url <domain>` via `std::process::Command`.
