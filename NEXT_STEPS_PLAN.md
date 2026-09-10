# 🗺️ Tunotron — Architectural Roadmap & Next Steps Plan

This document outlines the systematic engineering path from the current rock-solid, non-blocking baseline (`v0.2.0`) to a full sandboxed extension ecosystem powered by Lua 5.4 (`mlua`).

---

## 📊 Current State: Baseline v0.2.0 Achieved
- [x] **0.0% Idle CPU:** Event-driven architecture with Tokio reactor sleeping in `epoll_wait`.
- [x] **Throttled Render Pipeline:** Decoupled `mpv` IPC events (`time-pos`) from screen redraws; capped at 4 Hz during playback.
- [x] **Zero-Allocation Hot Path:** Borrowed string slices (`&str`) across table rows and static string badges.
- [x] **Non-Blocking Tag Scanning:** UI renders directory stems in `<1ms`; audio metadata probes asynchronously in background batches.
- [x] **Strict Canonical Jail:** Single chokepoint (`resolve_in_jail`) blocking symlink escapes and parent directory traversal above `music_root`.
- [x] **Ergonomic Controls:** True Fisher-Yates shuffle with `XorShift64` PRNG, shuffle history stack, scroll-offset-aware mouse hit-testing, and key sequence mismatch retry.
- [x] **Automated Test Suite:** 9 unit tests verifying jail boundaries, PRNG determinism, playback loop modes, key normalization, and metadata patching.

---

## 🎯 Phase 2: Pre-Plugin Architectural Guardrails (Immediate Next Step)

Before introducing the `mlua` runtime, the host architecture must be structurally insulated so that extension code cannot corrupt host memory, freeze the reactor, or bypass the security sandbox.

### 2.1 One-Way Mutation: Action Reducer & UI Snapshots
```
┌─────────────────┐       Action       ┌─────────────────┐     MpvCommand     ┌─────────────────┐
│ Host Input      │ ─────────────────> │  State Reducer  │ ─────────────────> │ Headless mpv    │
│ (Keys / Mouse)  │                    │  (Main Task)    │ ◄─── MpvEvent ──── │ (Async Actor)   │
└─────────────────┘                    └─────────────────┘                    └─────────────────┘
                                                │
                                                │ UiSnapshot (Immutable, Send)
                                                ▼
                                       ┌─────────────────┐
                                       │ Terminal Render │ (No &mut AppState)
                                       └─────────────────┘
```
- **Eliminate God Object Borrows:** Separate UI geometry (`UiGeom`: cursor position, table scroll state, rects) from domain state (`PlaybackState`, `TrackList`).
- **Snapshot Dispatch:** Reducer produces lightweight, immutable snapshots for rendering. Plugins never receive `&mut AppState`.
- **Plugin Action Isolation:**
  ```rust
  pub enum Action {
      // Core actions...
      Plugin {
          plugin_id: u32,
          name: Box<str>,
          payload: serde_json::Value,
      },
  }
  ```
  Unknown plugin actions emit a tracing warning and are safely ignored—never panicking or halting the player.

### 2.2 Shared Track References (`Arc<Track>`)
- **Problem:** When playing a directory or providing a track list to an extension, cloning vectors of `Track` clones 5 heap strings and a `PathBuf` per track.
- **Solution:**
  ```rust
  pub type TrackRef = std::sync::Arc<Track>;
  ```
  `browser_items`, `active_playlist`, and `playback.current_track` share immutable `TrackRef` pointers. Playlist creation becomes an O(1) atomic increment per track.

### 2.3 Window Stack Compositor
- Replace the boolean `show_help: bool` with an explicit window stack:
  ```rust
  pub struct Window {
      pub id: WindowId,
      pub title: String,
      pub view: ViewNode,
      pub source: WindowSource, // Host | Plugin(u32)
  }

  pub struct WindowManager {
      stack: Vec<Window>,
  }
  ```
- **Input Routing:** Top window consumes keyboard and mouse events until closed with `Esc` / `CloseTopWindow`. `Quit` always breaks through.
- **Mouse Shielding:** Hit-test clicks strictly against the top window's coordinates before propagating to background widgets.

---

## 🔌 Phase 3: Sandboxed Lua 5.4 Engine (`mlua`)

### 3.1 Strict Security & Capability Gating
Untrusted third-party scripts must run in a capability-secured environment:

| Permission Capability | Allowed Operations | Guard Implementation |
|---|---|---|
| `playback.control` | Play, Pause, Resume, Seek, Volume, Loop, Shuffle | Filtered at Reducer |
| `playback.queue` | Replace active playlist, jump to playlist index | Filtered at Reducer |
| `ui.overlay` | Push / pop floating modal windows | Window Manager check |
| `fs.jail_read` | Read audio files / directory contents | Enforced via `resolve_in_jail` |
| `fs.jail_write` | **DENIED** by default | Requires explicit user flag |
| `keys.bind` | Dynamic shortcut registration | Namespaced, cannot override `q`/`Esc` |
| `os.*` / `io.*` / `net.*` | **STRIPPED & BANNED** | Removed at Lua VM initialization |

### 3.2 Threading & Execution Model
- **Worker Isolation:** Each extension runs in its own Lua state inside a dedicated thread or `tokio::task::spawn_blocking` worker communicating exclusively via message channels.
- **Zero Reactor Blocking:** Lua computations never execute on the main asynchronous event loop or the mpv IPC channel.
- **Execution Limits:** Configure `lua.set_memory_limit(...)` (e.g. 16MB) and instruction step counters to terminate misbehaved or infinite-looping scripts.

### 3.3 Declarative UI Protocol (No Immediate Mode Leaks)
Plugins cannot directly call Ratatui or touch raw terminal frames. Instead, plugins emit declarative view structures:
```lua
function on_render()
    return {
        type = "modal",
        title = "Track Lyrics",
        width = 60,
        height = 20,
        content = {
            type = "paragraph",
            text = fetched_lyrics
        }
    }
end
```
The host compositor validates and renders the declarative tree within the floating window layer.

---

## 📦 Phase 4: Distribution & Packaging Pipeline

1. **Nix Flake & Home Manager Module:**
   - Standalone `flake.nix` providing `packages.default` and a NixOS / home-manager service module with `mpv` runtime bundling.
2. **Arch Linux (AUR / CachyOS):**
   - Clean `PKGBUILD` for `tunotron-bin` and `tunotron-git` targeting x86_64 and aarch64.
3. **Fedora / RPM:**
   - Specfile for Fedora Copr build targeting Fedora 40+.
4. **Zero Runtime Dependencies:**
   - Single release binary (< 5MB) requiring only standard `libc` and `mpv` installed on the host.

---

## 📅 Milestones Summary

| Milestone | Target Version | Focus |
|---|---|---|
| **Milestone 1** | `v0.2.0` (Completed) | Performance optimization, zero allocations, jail security, test suite |
| **Milestone 2** | `v0.3.0` | `TrackRef` (Arc), Action reducer separation, Window stack compositor |
| **Milestone 3** | `v0.4.0` | `mlua` integration, capability-gated sandbox, isolated worker threads |
| **Milestone 4** | `v0.5.0` | Plugin directory discovery (`~/.config/tunotron/plugins`), Declarative UI |
| **Milestone 5** | `v1.0.0` | Documentation, sample plugins (lyrics, discord RPC), distro packages |
