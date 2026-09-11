# 🗺️ Tunotron Extensions Roadmap & Architecture Masterplan

## 🚀 1. Current Velocity & Architectural Status

### Where Are We Right Now?
Tunotron has completed **Phase 1 (Core Engine & Jailed Browser)**, **Phase 2 (Pure Reducer & Monotonic Interpolation)**, and **Phase 3 (The 6 Extension Pillars & Host Foundation Hooks)**:

- **Bulletproof Foundation:** 66 unit & integration tests run in **0.17 seconds** with **0 compiler warnings** and **0 clippy warnings** under `-D warnings`.
- **0.0% Idle CPU:** Event-driven async runtime sleeps inside `epoll_wait`. Redraws are throttled to 1 Hz steady-state integer seconds only during active playback.
- **Microsecond Host Hooks:**
  - `Action::SeekAbsolute(f64)` (exact seek for A-B loops and lyrics syncing).
  - `Action::SetAudioFilter(String)` (dynamic mpv audio filter chains for equalizers and spatial audio).
  - `Action::ShowToast { message, duration_ms }` (sleek floating in-app alerts).
  - `Action::SetSlot` / `Action::ClearSlot` (named widget slots: `"topbar"`, `"player_extra"`).
  - `Action::Broadcast` & `PluginEvent::Custom` (cross-plugin pub-sub event bus).
  - **Zero-CPU Unix Socket IPC & CLI Controller:** `/run/user/<uid>/tunotron.sock` serves live Waybar JSON, SwayNC hooks, and headless CLI commands (`tunotron toggle`, `tunotron next`, `tunotron status`).
- **Release Footprint:** ~4.0 MB single standalone binary with embedded Lua 5.4, fat LTO, and zero external daemon dependencies.

**Current Velocity:** Development momentum is in the sweet spot. Because the core operates on a strict **pure reducer + cold action envelope + capability sandbox** seam, new capabilities and extensions do not require invasive refactors—they attach cleanly as modular contracts.

---

## 🧩 2. The 20 Extension Candidates Catalog

The system is designed so extensions are **mutually composable through soft dependencies and named slots**: they enhance each other when present, but gracefully degrade and function standalone when absent.

| # | Extension | Category | Required Capabilities | Inter-Extension Synergy |
|---|---|---|---|---|
| **1** | **Better Extensions** | Ecosystem / UI | `UiOverlay`, `PersistentStorage` | Visual settings manager, search bar, auto-generates sliders/toggles for all plugins |
| **2** | **Better Folders** | File Management | `FsJailRead`, `FsJailWrite` | Create folders, move/copy tracks, hide/deduplicate files inside music jail |
| **3** | **Better Home** | Dashboard / UI | `UiOverlay`, `FsJailRead` | Card-based homepage displaying folders as interactive playlist tiles with cover art |
| **4** | **Playlists** | Library / Media | `FsJailRead`, `FsJailWrite`, `PlaybackQueue` | Dynamic folder-playlists, custom artwork, 2x2 embedded metadata collage generation |
| **5** | **Better Layouts** | UI Compositor | `UiOverlay`, `PersistentStorage` | Reconfigurable panes, tab bar customization, modular viewport arrangement |
| **6** | **Lyrics** | Media / Stream | `Network`, `FsJailRead`, `UiOverlay` | Multi-provider fetcher, synchronized time-tagged scrolling, slot streaming into player |
| **7** | **Better Player** | Player / UI | `UiOverlay`, `PlaybackControl` | Modular player chassis hosting mini-visualizers, lyrics stream, knobs, mute, balance |
| **8** | **Better Notifications** | Alerts / IPC | `Notify`, `UiOverlay` | Rich desktop & in-app alerts with album art, playback progress, and interactive actions |
| **9** | **Sleep Timers** | Automation | `PlaybackControl`, `PersistentStorage`, `UiOverlay` | Configurable countdown, gentle volume fadeout, persistent duration across sessions |
| **10** | **Equalizer** | Audio DSP | `PlaybackControl`, `PersistentStorage` | Multi-band parametric frequency equalizer, presets (Bass Boost, Rock, Vocal, Flat) via mpv `af` |
| **11** | **Queue** | Playback Engine | `PlaybackQueue`, `UiOverlay` | Play-next queue, priority queue reordering, non-destructive temporary playlist |
| **12** | **Visualizer** | Visuals / DSP | `UiOverlay`, `Tick` | Terminal ASCII/braille spectrum bars, waveform scope in player or dedicated tab |
| **13** | **RSS Feeds** | Network / Media | `Network`, `UiOverlay`, `FsJailWrite` | Audio podcast / music feed parser, episode listing, notification alerts for new drops |
| **14** | **Section Loops** | Precision Audio | `PlaybackControl`, `PersistentStorage`, `KeyBind` | A-B repeat looping, millisecond precision seek, save loop markers to persistent state |
| **15** | **Now Playing** | UI Overlay | `UiOverlay`, `KeyBind` | Minimalist full-screen or card overlay with large typography, progress, and metadata |
| **16** | **Better Topbar** | Navigation / UI | `UiOverlay`, `PersistentStorage` | Customizable header line hosting slot widgets (timer, stats, equalizer preset) |
| **17** | **Stats** | Analytics / Profile | `PersistentStorage`, `Tick` | Listening statistics (play count, hours listened, favorite artists, scrobble log) |
| **18** | **Better Themes** | Aesthetics | `UiOverlay`, `PersistentStorage` | Dynamic color themes (Catppuccin, Nord, Gruvbox, Tokyo Night, OLED Black), truecolor engine |
| **19** | **yt-dlp Support** | Download / Stream | `Network`, `FsJailWrite` | Async audio stream/extraction from URLs directly into jailed music library |
| **20** | **7D / 8D Spatial Audio** | Audio DSP | `PlaybackControl`, `PersistentStorage` | Binaural 360° soundstage rotation, LFO panning (`apulsator`), room reverb simulation |

---

## 🎧 3. Deep Dive: Extension #20 — 7D / 8D Spatial Audio

### Concept & Psychoacoustics
7D/8D audio creates the illusion of sound rotating in an orbit around the listener's head with spatial depth. It achieves this through:
1. **Binaural Panning LFO:** Dynamic left-to-right phase modulation (e.g. 0.1 Hz sine wave for a 10-second orbital period).
2. **HRTF Spatial Diffusion:** Subtle stereo widening (`stereowiden`) and frequency filtering to simulate head-related transfer.
3. **Room Acoustic Ambience:** Low-ratio echo/reverb reflections to emulate spatial room volume.

### Technical Implementation via Tunotron Host Hook
Tunotron's `Action::SetAudioFilter(String)` maps directly to mpv's audio filter chain:
```lua
-- 8D Binaural Orbital Rotation Preset:
local filter = "lavfi=[apulsator=mode=sine:hz=0.08:amount=0.85,stereowiden=delay=20:feedback=0.3,aecho=0.8:0.88:40:0.3]"

-- Activate filter:
return {
    { action = "SetAudioFilter", filter = filter },
    { action = "SetSlot", slot = "topbar", content = "[8D Spatial: ON]" },
    { action = "ShowToast", message = "8D Audio activated (Headphones recommended)", duration_ms = 3000 }
}
```
- **CPU Overhead:** Handled natively by mpv/ffmpeg SIMD audio pipeline (0.0% overhead in Tunotron TUI).
- **Synergy:** Interacts with **#1 Better Extensions** for speed/depth sliders and **#16 Better Topbar** for status badge.

---

## 🏗️ 4. Phased Implementation Roadmap

### Phase 4.1: Pure Audio & Precision Tier (Current Sprint)
Extensions that utilize existing `PlaybackControl`, `KeyBind`, `PersistentStorage`, and `SetAudioFilter` hooks:
- [x] Host hooks deployed (`SeekAbsolute`, `SetAudioFilter`, `ShowToast`, `SetSlot`, Unix IPC).
- [ ] **#10 Equalizer:** Implement presets (Bass Boost, Treble, Vocal, Acoustic, Flat).
- [ ] **#20 7D / 8D Audio:** Implement orbital panning and spatial depth presets.
- [ ] **#14 Section Loops:** Implement A-B mark keybindings (`[` to mark A, `]` to mark B, `\` to clear).
- [ ] **#9 Sleep Timers v2:** Migrate to `SetSlot` countdown badge and `ShowToast` alert.

### Phase 4.2: Visual & Slot Ecosystem Tier
Extensions utilizing named slots and the inter-plugin event bus:
- [ ] **#16 Better Topbar:** Widget registry for topbar slots.
- [ ] **#8 Better Notifications:** Dual desktop `notify-send` + floating in-app toast alerts.
- [ ] **#15 Now Playing:** Clean floating modal with track details and artwork ASCII banner.
- [ ] **#18 Better Themes:** Theme switcher exporting Catppuccin, Nord, Gruvbox, and Dracula.
- [ ] **#1 Better Extensions:** Configuration menu reading declarative schemas (`manifest.settings`).

### Phase 4.3: Jailed Filesystem Operations Tier
Extensions requiring write capabilities inside the music root:
- [ ] Add `Capability::FsJailWrite` to host sandbox.
- [ ] Safe jailed directory creation, file moving, and track deletion.
- [ ] **#2 Better Folders** & **#4 Playlists**.

### Phase 4.4: Network & Remote Media Tier
Extensions requiring sandboxed network requests:
- [ ] Add `Capability::Network` with domain whitelisting and size limits.
- [ ] **#6 Lyrics Pipeline:** Remote fetching from LRCLIB / NetEase with fallback to local `.lrc`.
- [ ] **#13 RSS Feeds** & **#19 yt-dlp Integration**.
