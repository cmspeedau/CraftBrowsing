# Built-in Transfer Engine (v0.5.0)

CraftBrowsing's transfer engine unifies direct HTTP(S) downloads and
BitTorrent/magnet downloads behind one container-aware, privacy-controlled
manager. It is introduced in `v0.5.0` and is implemented by the
`craft-transfers` crate.

The torrenter project is the reference implementation for the transfer
algorithms. Its core modules are vendored and adapted into `craft-transfers`
with attribution. The torrenter MCP server is intentionally **not**
integrated.

---

## Scope

Included in v0.5.0:

- Segmented HTTP(S) downloads using 4 parallel byte-range parts.
- Magnet link downloads via librqbit.
- Pause, resume, stop, restart, and clear for both kinds.
- Concurrency cap (default 4) with queued jobs.
- Restore across browser restarts, including missing-file detection.
- Container tagging and filtering.
- Leech-by-default seeding for magnets.

Explicitly out of scope:

- torrenter MCP server integration.
- `.torrent` file import (planned follow-up).
- Container-specific torrent proxy routing (planned follow-up).

---

## Crate layout

```text
crates/craft-transfers/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── validate.rs      # magnet / HTTP URL validation
    ├── paths.rs         # download directory resolution
    ├── progress.rs      # shared progress + cancellation primitives
    ├── http.rs          # 4-part segmented HTTP downloader
    ├── torrent.rs       # librqbit magnet sessions
    └── jobs.rs          # persisted transfer state + restore logic
```

---

## Transfer kinds

| Kind   | Input                      | Engine    | Resume state                  |
| ------ | -------------------------- | --------- | ----------------------------- |
| `http` | `http://` / `https://` URL | reqwest   | `<dest>/.resume/` manifest    |
| `magnet` | `magnet:?xt=urn:...`     | librqbit  | `<dest>/<torrent-name>/` + `.torrenter-hash` marker |

Magnet links require a 32-character base32 or 40-character hex info hash.

---

## HTTP transfer behavior

1. Probe the remote size and range support.
2. If ranges are supported and the file is larger than one byte, download
   4 parallel byte-range parts.
3. Persist a resume manifest under `<dest>/.resume/`.
4. On pause, abort the live range body and continue from the saved offset.
5. On resume, continue from `<dest>/.resume/`.
6. If the server ignores ranges, fall back to a single stream.
7. Merge parts through a temporary file, then atomically rename to the
   final destination.
8. A short 206 response is an error, not a zero-filled hole.

---

## Magnet transfer behavior

1. Claim a sanitized subfolder under the download directory.
2. Write a `.torrenter-hash` marker associating the folder with the info hash.
3. Run DHT in memory only; do not persist peers.
4. **Leech mode (default):** never upload; stop the session at 100%.
5. **Share mode (explicit opt-in):** upload while downloading and keep
   seeding until the user presses Stop.
6. If payload files are missing on restore, mark the job stopped/missing.

Folder names are sanitized for path traversal and reserved names. Existing
folders are only reused when their hash marker matches the magnet.

---

## Privacy and security

- Seeding is **off by default**. Share mode is an explicit per-job opt-in and
  clearly discloses that the user's IP address is visible to peers.
- DHT runs in memory only; no peer or DHT state is persisted.
- A listening TCP port opens only in Share mode.
- Magnet downloads require a user gesture and confirmation prompt.
- Private browsing uses an in-memory database, so transfer history is
  ephemeral. Downloaded files still land on disk and the user is warned.
- Clear-on-exit and clear browsing data remove transfer history rows and,
  when configured, incomplete resume/temporary data.

---

## Container awareness

- Every transfer records the originating container ID.
- `craft://downloads` can filter by container.
- HTTP transfers preserve container cookie and proxy heritage where
  feasible.
- Magnet transfers are tagged with the container ID for history and UI.
  Container-specific torrent proxy routing is a follow-up.

---

## Concurrency and restore

- Default concurrency cap: 4 active transfers.
- Additional transfers are queued.
- On startup, transfer rows are loaded and disk state is probed.
- Missing HTTP resume parts or missing magnet payloads mark the job as
  stopped/missing and offer restart.

---

## UI

`craft://downloads` shows:

- Transfer kind badge (`HTTP` / `MAGNET`).
- Progress, bytes, total, speed, and ETA.
- Peer count for magnets.
- Upload speed/bytes for Share mode.
- Pause, resume, stop, restart, clear, open folder, and copy source URL.
- Search/filter across jobs.
- Queued state for jobs over the concurrency cap.

The toolbar download indicator covers all transfer kinds.

---

## Licensing

The vendored transfer algorithms originate from `danted4/torrenter`, which is
MIT-licensed. Vendored files retain attribution and license notices. The
torrenter MCP server is not included.