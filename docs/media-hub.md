# Global Media Hub & Picture-in-Picture (PiP) Scrubber

CraftBrowsing provides a unified media controller accessible from the browser toolbar, enabling users to monitor, scrub, and control all audio and video playback across tabs and containers, along with an advanced Picture-in-Picture (PiP) window.

---

## 1. Global Media Hub Toolbar Popup

A dedicated speaker/media icon in the navigation bar appears whenever any tab is producing audio or video output. Clicking it reveals the **Global Media Hub**:

```
+--------------------------------------------------+
| 🎵 Active Media Sessions                         |
+--------------------------------------------------+
| [▶] Rust Conf 2026 Keynote             [🔊 100%] |
|     youtube.com • Tab #3 (Personal Container)    |
|     04:15 [══════════════●────────────────] 42:10|
|     [⏮] [ ⏸ ] [⏭]   [🔁 Loop]   [🖼️ PiP Mode]     |
+--------------------------------------------------+
| [⏸] Synthwave Radio Stream             [🔊  60%] |
|     somafm.com • Tab #8 (Work Container)         |
|     Live [● REC]                                 |
|     [Mute Tab] [Close Tab]                       |
+--------------------------------------------------+
| [Mute All Tabs]                 [Open Media Hub] |
+--------------------------------------------------+
```

### Key Capabilities:
- **Cross-Tab Control:** Play, pause, skip, seek, and adjust volume for any tab without switching to it.
- **Audio Output Device Routing:** Select specific audio output hardware per container (e.g., route Work calls to headset, media to speakers).
- **One-Click Tab Mute / Kill:** Instantly silence noisy background tabs or auto-playing advertisements.

---

## 2. Advanced Picture-in-Picture (PiP) Window

CraftBrowsing's PiP window detaches HTML5 video elements into a dedicated, floating, always-on-top window managed natively by the OS window compositor.

### Features:
- **Always-on-Top Floating Shell:** Resizable with preserved aspect ratio, snapping to display corners.
- **Full Scrubber & Playback Controls:**
  - Interactive timeline scrubber with timestamp display.
  - Skip forward (+10s) / skip backward (-10s).
  - Playback speed toggle (0.5x, 1.0x, 1.25x, 1.5x, 2.0x).
- **Live Subtitle & Closed Caption (CC) Overlay:** Renders WebVTT subtitles directly inside the floating PiP window.
- **Multi-PiP Support:** Run multiple PiP windows simultaneously from different tabs or containers.
- **Automatic PiP on Tab Switch:** Optional setting (`craft://settings/media`) that auto-pops the active video into PiP when you switch tabs or minimize the browser.
