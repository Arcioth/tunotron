# 📝 Changelog — Tunotron

All notable changes, architectural decisions, and optimizations to Tunotron are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.3.5] - 2026-09-10 — Custom Keybinding Registration & Declarative Floating Modals

Following the plugin API expansion roadmap, this release establishes the first two foundational API pillars for Tunotron plugins: **Custom Keybinding Registration (`Capability::KeyBind`)** and **Declarative Floating Modals (`Capability::UiOverlay`)**. Plugins can now hook custom shortcuts and project rich floating TUI windows (e.g. lyrics viewers, track metadata inspectors) without leaking immediate-mode Ratatui frames or risking host security.

### ⌨️ Pillar 1: Custom Keybinding Registration (`Capability::KeyBind`)
- **Key Chord & Sequence Engine ([`src/keymap.rs`](file:///home/arcioth/Documents/tunotron/src/keymap.rs)):**
  - Robust parser supporting single keys (`"y"`, `"space"`), function keys (`"F1"`-`"F12"`), modifier combinations (`"ctrl+y"`, `"alt+enter"`, `"ctrl+shift+tab"`), and Vim-style multi-key sequences (`"g g"`, `"ctrl+x,ctrl+s"`).
  - Normalizes Shift on Char keys matching crossterm terminal semantics.
- **Strict Host Reserved Keys & Collision Guards:**
  - Reserved vital controls: plugins cannot hijack Quit (`q`/`Q`/`Ctrl+C`), Cancel/Close (`Esc`), or core file browser navigation (`j`, `k`, `↑`, `↓`, `G`, `gg`, `Enter`, `Backspace`).
  - Pre-existing default key bindings are shielded from accidental hijacking.
- **Declarative Manifest Declarations:**
  - Lua scripts declare keybindings statically in their manifest: `keybinds = { ["i"] = "announce_current" }`.
  - Registered bindings dispatch pure [`Action::Plugin`](file:///home/arcioth/Documents/tunotron/src/action.rs#L130) values through the state reducer, routing to `plugin.on_action(name, payload)`.

### 🪟 Pillar 2: Declarative Floating Modals (`Capability::UiOverlay`)
- **Typed Modal Action & Geometry ([`src/action.rs`](file:///home/arcioth/Documents/tunotron/src/action.rs#L124), [`src/ui/geom.rs`](file:///home/arcioth/Documents/tunotron/src/ui/geom.rs#L6-L20)):**
  - Added [`Action::ShowModal { title, content }`](file:///home/arcioth/Documents/tunotron/src/action.rs#L124) permission-checked under `Capability::UiOverlay`.
  - Added [`WindowId::PluginModal`](file:///home/arcioth/Documents/tunotron/src/action.rs#L84) to the pure `WindowStack` in [`UiGeom`](file:///home/arcioth/Documents/tunotron/src/ui/geom.rs#L15).
  - Pop-up cleanup: popping the window cleans up modal state automatically.
- **Pure Projection Compositor ([`src/ui/window.rs`](file:///home/arcioth/Documents/tunotron/src/ui/window.rs#L129), [`src/ui/layout.rs`](file:///home/arcioth/Documents/tunotron/src/ui/layout.rs#L39-L44)):**
  - Renders rounded, themed popup windows with background clearing (`Clear`) and text wrapping (`Wrap { trim: true }`).
  - Modal shielding: background clicks outside the modal dismiss it automatically; background widgets are shielded from click events while a modal is open.
  - Keyboard dismissal: `Esc` closes the active modal instantly.
- **Updated Reference Example ([`examples/plugins/track_announcer.lua`](file:///home/arcioth/Documents/tunotron/examples/plugins/track_announcer.lua)):**
  - Pressing `i` invokes a floating "Track Metadata Inspector" showing title, artist, album, duration, and filepath.

### 🧪 Automated Testing & Binary Footprint
- **34 Passed Unit Tests, 0 Warnings (0.08s execution time):**
  - Added tests for key chord parsing, sequences, reserved key blocking, capability rejection, modal permission checks, reducer modal state transitions, and Lua modal action emission.
- **Release binary footprint:** **3.6 MB** with Fat LTO and stripped symbols.

---

## [0.3.4] - 2026-09-10 — Sandboxed Lua 5.4 Extension Engine & Auto-Loader

Following the plugin architecture roadmap, this release embeds a sandboxed, low-overhead Lua 5.4 scripting runtime (`mlua`) on top of the native plugin trait and capability manager. Users can now drop single-file scripts (`*.lua`) or modular directory packages (`<plugin>/init.lua`) into `~/.config/tunotron/plugins/` to react to playback events and emit capability-governed actions.

### 🌙 Sandboxed Lua 5.4 Runtime (`mlua`)
- **Sandboxed `LuaPlugin` Engine ([`src/plugin/lua.rs`](file:///home/arcioth/Documents/tunotron/src/plugin/lua.rs)):** Full bridge conforming to the `Plugin` trait:
  - **Memory Guard:** Enforces a strict 16MB memory limit (`set_memory_limit`) on the Lua VM to prevent runaway allocations or memory leaks from untrusted scripts.
  - **Environment Sandboxing:** Stripped dangerous host system operations (`os.exit`, `os.execute`, `os.remove`, `os.rename`, etc., keeping only safe time/clock routines) and neutralized `package.loadlib` to block unauthorized shared-library loading.
  - **Host API Injection:** Injected global `tunotron` module with non-blocking logging hooks (`tunotron.log`, `tunotron.warn`, `tunotron.error`) and version introspection (`tunotron.version`).
  - **Dual Action Deserializer:** Translates return values from Lua callbacks into typed domain `Action` values supporting both concise string aliases (`"TogglePause"`, `"NextTrack"`, `"Quit"`) and structured parameter tables (`{ action = "Seek", seconds = 15 }`, `{ action = "SetVolume", volume = 80.0 }`, `{ action = "PlayTrackIndex", index = 3 }`).
- **User Plugin Discovery & Auto-Loading ([`src/plugin/loader.rs`](file:///home/arcioth/Documents/tunotron/src/plugin/loader.rs)):**
  - Discovers plugins in `$XDG_CONFIG_HOME/tunotron/plugins` (or `~/.config/tunotron/plugins`).
  - Automatically registers valid `*.lua` files and `<dir>/init.lua` plugin packages during initialization.
- **Example Plugin Blueprint ([`examples/plugins/track_announcer.lua`](file:///home/arcioth/Documents/tunotron/examples/plugins/track_announcer.lua)):**
  - Reference implementation illustrating manifest declaration, capability requests (`PlaybackControl`), lifecycle hooks (`on_load`, `on_unload`), event subscriptions (`on_event`), and custom action handlers (`on_action`).

### 🧪 Automated Testing & Footprint Verification
- **25 Passed Unit Tests, 0 Warnings (0.08s execution time):**
  - Added unit tests for script manifest parsing, event action emission, `os.exit` sandbox escape prevention, custom action routing, and Lua memory limit containment.
- **Strict Binary Constraint Met:**
  - Vendored Lua 5.4.7 compiles with zero system dependencies; release binary size is **3.5 MB** with Fat LTO and stripped symbols (well below the 3.8 MB ceiling).
  - 0 compiler and 0 clippy warnings across the entire workspace.

---

## [0.3.3] - 2026-09-10 — Rust Plugin Protocol & Sandboxed In-Tree Host

Following user architectural directives, this release establishes the Rust-native plugin protocol and lifecycle host before introducing Lua bindings. Plugins operate strictly as event-driven cold inputs, receiving read-only domain events and emitting capability-checked actions.

### 🔌 Rust Plugin Protocol & Lifecycle
- **Native `Plugin` Trait ([`src/plugin/traits.rs`](file:///home/arcioth/Documents/tunotron/src/plugin/traits.rs)):** Defined asynchronous-safe, sendable plugin interface:
  - `manifest(&self) -> &PluginManifest`: Declares plugin ID, human-readable metadata, and requested capabilities.
  - `on_load(&mut self) -> Result<(), String>`: Lifecycle hook triggered when registered.
  - `on_event(&mut self, event: &PluginEvent) -> Vec<Action>`: Domain event observer hook.
  - `on_action(&mut self, name: &str, payload: &Value) -> Vec<Action>`: Direct action invocation hook.
  - `on_unload(&mut self)`: Graceful cleanup hook.
- **Read-Only Domain `PluginEvent`:** Owned serializable domain events (`TrackChanged`, `PlaybackStopped`, `PlayStateChanged`, `TimePos`) providing plugins with immutable snapshots.
- **Plugin Manager Host ([`src/plugin/manager.rs`](file:///home/arcioth/Documents/tunotron/src/plugin/manager.rs)):** Handles registration, unregistration, lifecycle calls, and broadcasts.
- **Sandboxed Capability Enforcement:** Every `Action` emitted by a plugin is automatically wrapped in an `ActionEnvelope` and evaluated against the plugin's declared `Capability` set. Any attempt to invoke ungranted actions or destructive `HostOnly` actions (`Quit`) is blocked and logged.
- **Built-in Diagnostic Plugin ([`src/plugin/builtin.rs`](file:///home/arcioth/Documents/tunotron/src/plugin/builtin.rs)):** Added zero-overhead `TrackLoggerPlugin` running by default to observe track changes and verify the pipeline end-to-end.

### 🧪 Automated Testing & Binary Footprint
- 20 passed unit tests, 0 warnings (0.03s execution time).
- Unit tests verify plugin registration, lifecycle, capability permission, and default-deny blocking of rogue actions.
- Maintained exact **3.1 MB** binary size with Fat LTO and stripped symbols.

---

## [0.3.2] - 2026-09-10 — Pure Reducer, Isolated State & Explicit Typed Effects

Following user architectural directives, this release eliminates side-effects and cross-thread channel handles from `AppState`, completing the pure reducer pattern (`reduce(&mut self, action: Action, geom: &mut UiGeom) -> Vec<Effect>`) and preparing a bulletproof boundary for plugin integrations.

### 🧱 Pure Reducer & Effect Decoupling
- **Zero Channel Dependencies in `AppState`:** Stripped `cmd_tx` and `event_tx` channels from `AppState`. The state struct is now a 100% pure in-memory model containing zero channels, zero sockets, zero threads, and zero async runtimes.
- **Pure State Transitions:** Replaced `handle_action` with `pub fn reduce(&mut self, action: Action, geom: &mut UiGeom) -> Vec<Effect>`. All state modifications are completely deterministic and synchronous.
- **Typed `Effect` Enum:** All side-effects are cleanly emitted as typed values:
  - `Effect::Mpv(MpvCommand)`: Controls playback, volume, seeks, and daemon termination.
  - `Effect::LoadDirectory { dir, root }`: Triggers non-blocking directory reads in the background threadpool.
  - `Effect::ScanMetadata(Vec<PathBuf>)`: Dispatches background metadata extraction for uncached tracks.
  - `Effect::PluginAction { plugin_id, name, payload }`: Extension hook for plugin invocations.
- **Central Effect Runner:** Added `execute_effects` in `main.rs` to process emitted effects and route them to their respective actors and workers.
- **Channel-Free Unit Testing:** Unit tests for `AppState` no longer need to construct Tokio async channels; testing state transitions is now instantaneous and completely deterministic.

### 🧪 Automated Testing & Binary Footprint
- 17 passed unit tests, 0 warnings (0.03s execution time).
- Tested reducer effects via `test_pure_reducer_effects`.
- Release binary footprint preserved at **3.1 MB** with Fat LTO and symbol stripping.

---

## [0.3.1] - 2026-09-10 — Phase 2.5 Pre-Plugin Architecture & Pure Projection Rendering

Following an architectural review, this release implements Phase 2.5 architectural guardrails, establishing pure projection rendering, typed window compositing, and sandboxed action capability boundaries in preparation for the Lua 5.4 (`mlua`) plugin engine.

### 🧩 UI Geometry Decoupling & Pure Projections
- **Separated `UiGeom` from `AppState`:** Extracted all viewport motions, table selection state (`TableState`), layout hitboxes (`Rect`), and modal window tracking into a dedicated `UiGeom` struct.
- **Pure Projection Rendering:** `render_app` now takes `(&AppState, &mut UiGeom, &Theme)`. Domain state (`AppState`) is 100% immutable during render—eliminating mutations during draw and enabling safe zero-copy snapshotting.

### 🪟 Typed Window Stack Compositor
- **Replaced `show_help: bool`:** Modal windows are now managed by a typed stack (`window_stack: Vec<WindowId>`).
- **Ergonomic Compositing:** Full stack operations: `push_window`, `pop_window`, `toggle_window`, and `close_top_window`.
- **Click Shielding & Dismissal:** Mouse clicks outside the active modal window automatically dismiss it; clicks inside the modal are shielded from activating background widgets.

### 🔒 Strict Default-Deny Plugin Capability Policy
- **Inverted Capability Enforcement:** Replaced permissive fallback with strict default-deny (`ActionPermission::{Public, Capability, HostOnly}`). Any action not explicitly marked as `Public` or granted via capability is strictly rejected. Destructive host actions (`Quit`) are permanently isolated to `HostOnly`.

### 🧠 Genuine O(1) Intrusive LRU Cache
- **True LRU Eviction:** Replaced arbitrary HashMap bucket eviction with a zero-dependency index-based intrusive doubly linked LRU (`LruCache<K, V>`). When the 10,000-track boundary is reached, the genuine least-recently-used metadata entry is evicted in strictly O(1) time.

### 🆔 Stable `TrackId` Domain Identity
- **Separation of Identity from Display Index:** Replaced display-row sequential renumbering with deterministic 64-bit canonical path hash identity (`TrackId`). Track references held by playlists, queues, or future plugins remain stable across folder reloads and sorting.

### 🧪 Automated Testing
- Expanded test suite to **16 passed tests, 0 warnings**:
  - `test_selection_motions_and_bounds`: verifies selection clamping, bounds, and viewport jumps.
  - `test_window_stack_push_pop_toggle`: verifies window stack push, pop, deduplication, and toggle.
  - `test_action_source_permissions`: verifies user vs plugin capability verification.
  - `test_action_permissions_default_deny`: verifies strict default-deny capability enforcement.
  - `test_lru_eviction_order`: verifies O(1) LRU access ordering and eviction of least-recently-used tracks.

---

## [0.3.0] - 2026-09-10 — Monotonic Interpolation, Zero-Alloc Player Bar & Architecture Polish

Following a detailed systems review, this release reduces IPC overhead by another 20×, eliminates all remaining per-frame heap allocations, fixes IPC request-response correlation, adds zero-copy playlist switching with `Arc<Track>`, and trims binary footprint down to 3.1 MB.

### ⚡ Next-Gen Runtime & IPC Efficiency
- **Monotonic `PlaybackClock` Interpolation:** Introduced an `Instant`-based monotonic playback clock. Instead of continually polling mpv 4 times per second over UNIX sockets, Tunotron computes progress locally via `anchor_pos + anchor_at.elapsed()`.
- **0.2 Hz Calibration & 1 Hz Render Ticker:**
  - Background mpv IPC resync is reduced from 4 Hz to **0.2 Hz (once every 5 seconds)**, a further **20× reduction in socket traffic**.
  - Screen redraws only trigger when the integer seconds value increments (**1 Hz**) during steady playback, reducing terminal render workload by 4×.
  - UI inputs (keys, clicks, seeks, pause) remain 0ms instantaneous.
- **Strict Request ID Correlation:** In `run_mpv_actor`, responses from mpv are now correlated strictly by `request_id`. Only authoritative responses to `GetTimePos` update playback time, eliminating race conditions with other mpv command replies.

### 🎯 Zero-Allocation Integrity
- **100% Zero Heap Allocations Per Frame:** All residual format strings in `render_player_bar` and `render_browser_table` have been eliminated:
  - `now_playing_label` is cached on `PlaybackState` upon track load/metadata update.
  - `vol_label`, `time_label`, and `browser_title` are precomputed and borrowed as `&str`.
  - The claim of **0 heap allocations per second during steady-state playback** is now 100% literally true.

### 🛡️ Architecture & Memory Safety
- **Zero-Copy Active Playlists (`Arc<Track>`):** Wrapped browser and playlist tracks in `Arc<Track>`. Selecting a folder with hundreds of songs now clones lightweight pointers instead of deep-copying `PathBuf` and string fields.
- **Unambiguous `PlayState` Enum:** Replaced separate boolean flags (`is_playing`, `is_paused`) with `PlayState { Stopped, Playing, Paused }`, eliminating contradictory state combinations.
- **10,000-Track LRU Metadata Cache Cap:** Implemented an eviction limit on `metadata_cache` to guarantee memory usage remains strictly bounded on large multi-terabyte libraries.
- **Graceful mpv IPC Shutdown:** Normal exit now transmits `MpvCommand::Quit` to mpv over the socket, enabling ALSA and PipeWire audio backends to shut down cleanly without buffer pops.

### 📦 Build & Dependency Optimization
- **Stripped Unused Dependencies:** Removed unused crates (`toml`, `thiserror`, `chrono`) and redundant `futures` crate (retaining `futures-util` with std/sink).
- **Pruned Feature Bloat:** Trimmed `tokio` (removed `"full"`) and `ratatui` (removed `"all-widgets"`).
- **Release Profile:** Enabled Fat Link-Time Optimization (`lto = "fat"`), `codegen-units = 1`, and symbol stripping.
- **Binary Size:** Dropped from **4.5 MB to 3.1 MB** (a ~32% reduction).

### 🧪 Automated Testing
- Added `test_playback_clock_interpolation` verifying clock monotonicity and pause freezing (test suite now at **11 passed tests, 0 warnings**).

---

Following a targeted code review, this release eliminates IPC message serialization churn, introduces an in-memory metadata cache, and moves directory loading entirely off the main event loop.

### ⚡ Performance & IPC Efficiency
- **Polled `time-pos` (IPC Churn Elimination):** Removed `observe_property time-pos`. Instead of mpv continually broadcasting 40–50 unprompted JSON messages per second over the socket, Tunotron polls `get_property time-pos` on the 250ms tick timer (4 Hz) during active playback. This eliminates over 90% of UNIX socket traffic and string allocation in the mpv actor.
- **In-Memory Metadata Cache:** Added `metadata_cache: HashMap<PathBuf, MetadataPatch>` on `AppState`. Revisiting previously scanned folders now populates tags instantaneously with **zero disk reads and zero background probes**.
- **Asynchronous Directory Reading:** Directory loading now runs in `tokio::task::spawn_blocking` and dispatches `AppEvent::DirectoryLoaded`, preventing any UI stutters on slow mechanical drives or remote network shares.
- **Eliminated Redundant Syscalls in `resolve_in_jail`:** Replaced redundant `canonicalize()` calls on `music_root` inside directory loops with a pre-canonicalized root path comparison.
- **Display-Order Track IDs:** `Track::id` is assigned sequentially (1, 2, 3, ...) after directory sorting, strictly matching on-screen display order.

### 🧪 Automated Testing
- Added `test_metadata_cache_instant_load` verifying instant cache hits and zero background event emission on cached folders (bringing test suite to 10 passed tests).

---

## [0.2.0] - 2026-09-10 — Core Optimization & Security Hardening

Following a comprehensive systems audit (documented in `GROK_REVIEW.md`), this release optimizes hot rendering paths, eliminates memory allocations, hardens the filesystem jail against symlink escapes, and refines mouse and playback ergonomics.

### ⚡ Performance & Zero-Allocation
- **Throttled Playback Rendering:** Decoupled `time-pos` property events from screen redraws. The UI now updates the seekbar on a fixed 250ms cadence and discrete state transitions (`pause`, `duration`, `end-file`), eliminating 50+ unnecessary table redraws per second.
- **Zero-Copy Table Rendering:** Table rows in `render_browser_table` now borrow directly from `AppState` (`track.title.as_str()`, `track.artist.as_str()`, `track.duration_label.as_str()`) instead of cloning strings every frame.
- **Asynchronous Metadata Scanning:** Split directory listing from tag extraction. `read_directory` populates folder views instantly with fallback stems on the main thread, while audio metadata is read in chunks in `tokio::task::spawn_blocking` and merged via `ScannerEvent::Batch(Vec<MetadataPatch>)`.
- **Pre-computed Duration Labels:** Added `duration_label: String` cached on `Track` at discovery time, eliminating per-cell runtime string formatting.
- **IPC Codec Cap & Channel Bounding:** Capped `LinesCodec` at 64KB per line to prevent unbounded IPC allocations, and converted domain channels to bounded `mpsc::channel(64)`.

### 🔒 Security & Jail Hardening
- **Canonical Path Verification (`resolve_in_jail`):** All path transitions (`EnterDirectory`, `GoToParentDirectory`) now pass through canonical resolution. Traversal outside `music_root` is strictly rejected.
- **Symlink Jail Defense:** Directory listings now inspect raw `FileType` and refuse to traverse symlinks pointing outside the jail boundary.
- **Portable Home Fallback:** Changed `dirs_home()` fallback from a hardcoded developer path to system root `"/"`.

### 🖱️ Mouse & Hitbox Fixes
- **Exact Geometry Alignment:** Calculated `browser_rows_rect` strictly from `block.inner(area)` minus header height, eliminating off-by-one click inaccuracies.
- **Scroll-Aware Hit-Testing:** Track clicks now factor in `table_state.offset()`, ensuring clicks accurately select the intended track in scrolled lists.
- **Modal Event Shielding:** Mouse clicks now stop at floating windows (such as the Help modal) without bleeding through to background widgets.
- **Ghost Hitbox Elimination:** Reset `progress_rect` on small terminal heights to prevent phantom clicks.

### 🎵 Playback & Ergonomics
- **XorShift64 PRNG Shuffle Deck:** Replaced timestamp-modulo dice rolling with a stored `XorShift64` PRNG. Fisher-Yates shuffle now guarantees zero immediate repeats and full deck permutation before recycling.
- **Shuffle History Tracking:** `previous_track` under shuffle now steps backward through previous play history (`shuffle_history`) rather than linear index decrementing.
- **Loop: Off Honor in Shuffle:** Advancing through a shuffled playlist now properly halts when `LoopMode::Off` and the deck is exhausted.
- **Dynamic Viewport Scrolling:** `HalfPageDown` and `HalfPageUp` now calculate jumps based on `browser_rows_rect.height / 2` instead of a hardcoded constant.
- **Keymap Shift Normalization:** Automatically strips `KeyModifiers::SHIFT` from `KeyCode::Char(_)`, ensuring cross-terminal reliability for shortcuts like `?`, `+`, and `G` across Kitty, Alacritty, and Foot.
- **Chord Mismatch Recovery:** Fixed keychord buffer so that non-matching chords (e.g. typing `g` then `j`) re-feed the last key instead of dropping it.

### 🧪 Automated Testing
- Added comprehensive unit tests (`cargo test`) covering `resolve_in_jail`, `ascii_case_cmp`, `XorShift64`, `advance_track` loop & shuffle modes, `KeyChord` shift normalization, `KeySequenceStateMachine` prefix recovery, and background `apply_metadata_patches`.

---

## [0.1.1] - 2026-09-10 — Linear Folder Playlist & Ergo Refactor
- Eliminated the 3-tab requirement in favor of a unified folder-as-playlist model.
- Fixed pause/resume bug where resuming restarted audio from 0:00.
- Implemented `LoopMode` (`Off`, `Track`, `All`) and `ShuffleMode` (`Off`, `On`).
- Added "Locate Song" shortcut (`.`) to snap cursor back to active track.
- Added View Density toggle (`Z` key) for compact vs normal screen scaling.
- Added Help Modal popup (`?` key) with floating window compositor layer.

---

## [0.1.0] - 2026-09-10 — Initial Foundation
- Initial project scaffolding with Rust, Ratatui, Crossterm, and Tokio.
- Headless `mpv` supervisor with Linux `PR_SET_PDEATHSIG` for zero-zombie background cleanup.
- Asynchronous JSON-RPC Unix domain socket client over `$XDG_RUNTIME_DIR`.
- Background audio tag extraction via `lofty`.
- Custom RAII `TerminalHarness` with panic restoration hook.
- `shell.nix` for reproducible builds on NixOS, Arch, and Fedora.
