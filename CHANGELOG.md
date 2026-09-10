# 📝 Changelog — Tunotron

All notable changes, architectural decisions, and optimizations to Tunotron are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
