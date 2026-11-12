# Native Backup, Encrypted Profile Export & Portability

CraftBrowsing guarantees complete user sovereignty over all browser state, configuration, and credentials. Users can create portable, zero-knowledge encrypted vault archives for offline backups and run the browser in zero-trace portable mode.

---

## 1. Encrypted Profile Vaults (`.craftvault`)

Users can create single-file, password-protected snapshots of their entire browser environment under `craft://settings/backup`.

### Cryptographic Architecture:
- **Key Derivation:** Argon2id (memory-hard, resistant to GPU/ASIC brute-force).
- **Authenticated Encryption:** ChaCha20-Poly1305 AEAD.
- **Payload Format:** Streaming compressed tarball (`tar.zst` with Zstandard compression) encrypted inside an authenticated envelope.

```
+--------------------------------------------------------------------------+
| .craftvault Header: [ Magic: "CRAFTVAULT" ] [ Version: 1 ]               |
| Argon2id Salt (32 bytes) | Nonce (12 bytes)                              |
+--------------------------------------------------------------------------+
| Encrypted Ciphertext Payload (ChaCha20-Poly1305):                        |
|   ├── bookmarks.sqlite                                                   |
|   ├── history.sqlite                                                     |
|   ├── containers.json (Contextual Identity definitions & icons)           |
|   ├── feeds.sqlite (RSS/Atom subscriptions & read states)                |
|   ├── adblock_rules.dat (Custom user filter rules & subscriptions)       |
|   ├── preferences.json (User settings & theme overrides)                 |
|   └── extensions/ (Installed WebExtensions unpacked manifests & storage) |
+--------------------------------------------------------------------------+
| Poly1305 Authentication Tag (16 bytes)                                   |
+--------------------------------------------------------------------------+
```

### Granular Backup Selector:
When generating or restoring a vault, users can selectively include or exclude:
- [x] Bookmarks & Reading List
- [x] Browsing History
- [x] Multi-Account Container Definitions
- [x] RSS / Atom Feed Subscriptions
- [x] Custom Adblock Filter Rules & Whitelists
- [x] Browser Themes & Settings
- [ ] Cookies & Active Sessions (Off by default for security)
- [ ] Installed Extensions & Their Local Storage

---

## 2. Zero-Trace Portable Mode (`--portable`)

CraftBrowsing can be launched as a fully self-contained portable application (e.g., from a USB drive) without modifying host OS registries or creating files in `%AppData%`, `~/Library/Application Support`, or `~/.config`.

### Behavior:
1. **Invocation:** Pass the `--portable` CLI flag or create an empty file named `portable.dat` in the same directory as the executable.
2. **Data Confinement:**
   - All profile data, SQLite databases, caches, and logs redirect to `./craft_data/`.
   - Temporary files write strictly to `./craft_data/temp/`.
3. **Zero Host Traces:** On exit, zero registry keys, cache folders, or temp files remain on the host machine.
4. **Host Machine Isolation:** Hardware acceleration falls back cleanly to software rendering if target host GPU drivers are unavailable.
