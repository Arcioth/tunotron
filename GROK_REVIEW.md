# Tunotron — Grok Optimization & Architecture Review

Reviewed against `GROK_REVIEW_PROMPT.md` on 2026-09-10. Source of truth is the tree under `src/`, not the README claims. Release binary on disk: **4 585 576 bytes** (`target/release/tunotron`). There are **zero tests**.

**Verdict:** The skeleton is right (Action reducer, mpv actor on a UNIX socket, RAII terminal, folder = playlist, jail *intent*). It is **not** yet ready for an `mlua` sandbox, and a few hot-path choices will fight you the moment a folder has more than a couple of dozen tracks or a Lua plugin starts emitting actions. Fix the items in §1 before writing the extension engine. §2 is cheap cleanup. §3 is the contract the Lua layer needs.

---

## Already solid (do not “fix”)

- `MpvSupervisor::spawn` uses the correct Linux orphan pattern: `PR_SET_PDEATHSIG` **then** `getppid() != parent_pid` → `_exit(1)` inside `pre_exec`. That *is* the textbook race fix. `prctl` / `getppid` / `_exit` are async-signal-safe; the closure does not allocate.
- `TerminalHarness` restores the terminal on `Drop` and from the panic hook *before* printing. Keep this.
- `Action` as a pure enum reducer (`handle_action`) is the right Lua seam. Do not replace it with Lua writing `AppState` fields.
- Domain-event `try_recv` drain after the first `event_rx.recv()` is the right *shape* for coalescing. It is just applied to the wrong events (see §1.1).
- `Path::starts_with` is component-wise, so `/music-evil` does not slip past a root of `/music`. Keep using path components, not string prefixes.
- `end-file` only advances on `reason == "eof"`. That correctly ignores `stop` / `redirect` from `loadfile replace`.

---

## 1. Critical optimizations (high impact / low risk)

### 1.1 Do not redraw the world on every `time-pos` (CPU + allocations)

`observe_property` on `time-pos` makes mpv emit a `property-change` many times per second during playback. Each one:

1. Allocates `MpvEvent::PropertyChange { name: String, data: Value, ... }` in the actor.
2. Goes through an **unbounded** `mpsc`.
3. Sets `should_render = true` in `main.rs`.
4. Rebuilds **every** table `Row` in `render_browser_table` (including `title.clone()` / `artist.clone()` / `formatted_duration()`).

The 250 ms ticker already exists to refresh the gauge. Observing `time-pos` at full rate plus dirtying the UI on each event makes the ticker redundant and makes the “0 % CPU when idle” claim false the moment a file is playing. Coalescing only helps when events pile up *during* a draw; if the loop keeps up, you pay a full redraw per event.

`LinesCodec::new()` uses `max_length: usize::MAX` (tokio-util 0.7.19). A single huge IPC line can grow without bound.

**Do this:**

1. Stop observing `time-pos` (and unused `media-title`).
2. On the existing ticker, ask mpv for `time-pos` (or interpolate locally from last known position + `Instant`).
3. Dirty-render on ticker / user input / discrete mpv events (`end-file`, `pause`, `duration`, `idle`), **not** on `time-pos`.
4. Cap the codec; bound the event channel.

`src/audio/mpv.rs` — init + codec:

```diff
-    let mut reader = FramedRead::new(read_half, LinesCodec::new());
-    let mut writer = FramedWrite::new(write_half, LinesCodec::new());
+    let mut reader = FramedRead::new(read_half, LinesCodec::new_with_max_length(64 * 1024));
+    let mut writer = FramedWrite::new(write_half, LinesCodec::new_with_max_length(64 * 1024));

     let init_cmds = [
-        r#"{"command":["observe_property",1,"time-pos"]}"#,
         r#"{"command":["observe_property",2,"pause"]}"#,
         r#"{"command":["observe_property",3,"duration"]}"#,
-        r#"{"command":["observe_property",4,"media-title"]}"#,
         r#"{"command":["observe_property",5,"volume"]}"#,
     ];
```

Add a `MpvCommand::GetTimePos` (or generic `GetProperty`) and, in the actor, turn the matching `MpvResponse` into a synthetic `PropertyChange { name: "time-pos", ... }` **or** a dedicated `MpvEvent::TimePos(f64)`.

`src/main.rs` — ticker asks, `time-pos` does not dirty:

```diff
   // Branch 2: Domain Events (mpv IPC events)
   Some(domain_event) = event_rx.recv() => {
       match domain_event {
           AppEvent::Mpv(mpv_ev) => {
-              app.handle_mpv_event(mpv_ev);
-              should_render = true;
+              let dirty = app.handle_mpv_event(mpv_ev);
+              should_render |= dirty;
           }
           _ => {}
       }
       while let Ok(pending) = event_rx.try_recv() {
           match pending {
-              AppEvent::Mpv(mpv_ev) => app.handle_mpv_event(mpv_ev),
+              AppEvent::Mpv(mpv_ev) => {
+                  should_render |= app.handle_mpv_event(mpv_ev);
+              }
               _ => {}
           }
       }
   }

   // Branch 3: Guarded Seekbar Timer
   _ = ticker.tick(), if app.is_playing() => {
+      let _ = app.cmd_tx.send(MpvCommand::GetTimePos);
       should_render = true;
   }
```

`handle_mpv_event` should return `bool` (dirty). `time-pos` updates `current_time_sec` and returns `false` if you still observe it as a fallback; `pause` / `end-file` / `duration` / `idle` return `true`.

Also switch `event_tx` to a **bounded** channel (`mpsc::channel(64)`). If the UI is stuck in a lofty-heavy redraw, unbounded `time-pos` (today) or a chatty plugin (tomorrow) is an OOM.

**Backpressure on the socket:** `run_mpv_actor` never applies any. UNIX sockets will eventually block the writer in mpv, which is the only real backpressure you get. That is acceptable *once* the actor no longer forwards 50–100 property-change allocations/sec onto an unbounded channel. Prefer dropping stale `time-pos` values (keep latest only) over queueing them.

`tokio::select!` without `biased;` randomly shuffles poll order, so the reader will not strictly starve `cmd_rx`. After this change the reader is quiet enough that it does not matter. If you later re-enable a noisy observe, poll `cmd_rx` first with `biased;`.

---

### 1.2 Stop cloning strings in the render loop

`render_browser_table` maps **the entire** `browser_items` vec, not the visible window. Ratatui’s `Table` requires that (it uses `TableState.offset` internally), so you cannot virtualize without switching widgets. You *can* stop allocating per cell per frame.

`Cell` / `Text` accept `Into<Cow<'a, str>>`. `track.title` and `track.artist` already live on `AppState`; borrow them. Directory names too.

`src/ui/layout.rs`:

```diff
                 BrowserEntry::Directory { name, .. } => {
                     Row::new(vec![
                         Cell::from(" 📁"),
-                        Cell::from(format!("{}/", name)),
+                        Cell::from(name.as_str()),
                         Cell::from("<Folder>"),
                         Cell::from(""),
                     ])
                     .style(Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))
                 }
                 BrowserEntry::AudioTrack(track) => {
                     // ...
                     Row::new(vec![
                         Cell::from(status_icon),
-                        Cell::from(track.title.clone()),
-                        Cell::from(track.artist.clone()),
-                        Cell::from(track.formatted_duration()),
+                        Cell::from(track.title.as_str()),
+                        Cell::from(track.artist.as_str()),
+                        Cell::from(track.duration_label.as_str()),
                     ])
```

Cache the duration label on the `Track` at metadata-read time (it only changes when `duration_sec` changes, which is once per file plus one mpv `duration` event for the *playing* track):

```rust
// library/track.rs
pub struct Track {
    // ...
    pub duration_sec: f64,
    pub duration_label: String, // "03:41", filled by Scanner / Track::new
}

impl Track {
    pub fn set_duration(&mut self, sec: f64) {
        self.duration_sec = sec;
        self.duration_label = format_mmss(sec);
    }
}
```

Header/player-bar `format!` calls are once per frame, not once per row. Leave them until §2.

---

### 1.3 Mouse hitboxes are wrong (off-by-one **and** ignores scroll)

Two independent bugs in `src/main.rs` × `src/ui/layout.rs`.

**Geometry.** `browser_rect` is `chunks[1]`, the **outer** block including borders. Ratatui `Table` then does `block.inner(area)` (top border = 1 row) and splits `header_height` (1) + `header.bottom_margin` (0 compact / 1 comfortable). First data row is therefore at:

| density | actual first data row | code `header_offset` |
|---|---|---|
| Compact | `y + 2` | `1` |
| Comfortable | `y + 3` | `2` |

Clicks land one row *above* the track the user sees. The column header itself is a hit target.

**Scroll.** `clicked_row` is a visual row. `SelectIndex` treats it as an index into `browser_items`. After the table has scrolled (`TableState::offset()` — public in ratatui 0.29), click-to-select is wrong by `offset`.

**u16 overflow.** `p_rect.x + p_rect.width` can wrap. `Rect::contains` uses `saturating` edges. Use it.

**Stale rects.** If `render_player_bar` returns early (`inner.height < 2`), `progress_rect` is not updated. A later click seeks against the previous frame’s coordinates.

**Help modal.** Mouse events are not clipped. Clicks pass through the help window into the table/seekbar.

**Fix:** compute the *rows* rect in the same place you render, store that, hit-test with `contains`, add `offset`.

`src/app.rs` — store the rows rect, not the outer block:

```rust
pub browser_rows_rect: Rect, // data rows only (inside border, below header)
pub progress_rect: Rect,
```

`src/ui/layout.rs` — derive from `Block::inner`, do not duplicate magic numbers in `main.rs`:

```rust
fn render_browser_table(frame: &mut Frame, area: Rect, state: &mut AppState, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .title(format!(" Music Browser ({} items) ", state.browser_items.len()));

    let inner = block.inner(area);
    let header_h: u16 = 1 + if state.density == ViewDensity::Compact { 0 } else { 1 };
    state.browser_rows_rect = Rect {
        x: inner.x,
        y: inner.y.saturating_add(header_h),
        width: inner.width,
        height: inner.height.saturating_sub(header_h),
    };
    // ... rest unchanged, still render Table with .block(block) on `area`
}
```

`src/main.rs`:

```rust
use ratatui::layout::Position;

MouseEventKind::Down(MouseButton::Left) => {
    if app.show_help {
        // modal eats clicks; do not hit-test widgets underneath
        continue; // or just skip
    }
    let pos = Position { x: mouse.column, y: mouse.row };
    if app.progress_rect.contains(pos) {
        let relative_x = mouse.column.saturating_sub(app.progress_rect.x) as f64;
        let denom = app.progress_rect.width.max(1).saturating_sub(1).max(1) as f64;
        let ratio = (relative_x / denom).clamp(0.0, 1.0);
        app.handle_action(Action::SeekRatio(ratio));
        should_render = true;
    } else if app.browser_rows_rect.contains(pos) {
        let visual = (mouse.row.saturating_sub(app.browser_rows_rect.y)) as usize;
        let idx = app.table_state.offset() + visual;
        app.handle_action(Action::SelectIndex(idx));
        should_render = true;
    }
}
```

Always assign `progress_rect` (empty `Rect` if the bar does not fit) so a shrink-resize cannot leave a ghost hitbox.

`HalfPageDown` / `HalfPageUp` hardcode `15`. Use `browser_rows_rect.height.max(1) as usize` (or `/ 2` if you want an actual half page). Same class of bug: geometry lives in two places.

Resize itself is fine: `CrosstermEvent::Resize` only needs `should_render = true`; `Terminal::draw` re-queries size. No extra work there.

---

### 1.4 Jail is lexical; directory listing follows symlinks

`AppState::new` canonicalizes `music_root`. After that, nothing is canonicalized.

`read_directory` uses `path.is_dir()`, which **follows** symlinks. A folder `~/Music/escape → /etc` is listed as a directory. `Action::EnterDirectory` assigns `self.current_dir = path` with **no** jail check (the check exists only for `ParentDir`). The UI then lists `/etc` (via the link). Playback `LoadFile` will take whatever path is in the `Track`.

`Scanner::scan_directory_in_background` (dead, but the obvious Lua library hook) uses `WalkDir::new(&dir).follow_links(true)`. Do not wire that up as-is.

`GoToParentDirectory` compares `current_dir.starts_with(&self.music_root)` without canonicalizing. That is OK only if `current_dir` never contains `..` or a symlink. Today it can.

**Single chokepoint** — use it from navigation, scanner, `LoadFile`, and later Lua `fs`:

```rust
fn resolve_in_jail(root: &Path, candidate: &Path) -> Option<PathBuf> {
    let canon = candidate.canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    if canon == root || canon.starts_with(&root) {
        Some(canon)
    } else {
        None
    }
}
```

When listing, do not follow directory symlinks:

```rust
let ft = match entry.file_type() {
    Ok(ft) => ft,
    Err(_) => continue,
};
if ft.is_symlink() {
    continue; // or resolve_in_jail(entry.path()) and skip on None
}
if ft.is_dir() { /* Directory */ }
else if ft.is_file() && Track::is_audio_file(&path) { /* track */ }
```

Reject `EnterDirectory` unless `resolve_in_jail` succeeds. This is also the Lua filesystem gate.

---

### 1.5 `read_directory` blocks the UI thread on lofty

Every `EnterDirectory` / `ReloadDirectory` / startup calls `Scanner::read_metadata_sync` inline on the tokio main task. `Probe::open` + tag read per file. A 200-track FLAC folder will freeze input, mpv events, and redraws for seconds. The 250 ms ticker cannot run. This is the real startup/navigation cost; the binary size is not.

You already have the right design, unused: `Scanner::scan_directory_in_background` + `AppEvent::Scanner`. `main.rs` currently swallows those events (`_ => {}`).

**Split listing from tags:**

1. On the UI thread: `read_dir`, file_type, names, `Track::new` fallback title = stem. Sort. Show immediately.
2. `spawn_blocking`: lofty per file, send `ScannerEvent::Batch` of `(path, title, artist, album, duration, track_number)`.
3. Reducer matches on `path` and patches `browser_items` / `active_playlist` in place. Dirty-render.

Do **not** `follow_links(true)` when you wire this up. Drop the `track.clone()` into both `batch` and `tracks` in the current scanner (`scanner.rs:34-35`); move the `Track` into one of them.

---

### 1.6 Shuffle is not Fisher-Yates, and it ignores Loop/Prev

`pseudo_random` is `SystemTime::now().subsec_nanos() % max` called in a tight loop in `reset_shuffle_deck`. Consecutive calls often share the same nanosecond, so `j` is constant and the “shuffle” barely permutes.

When `ShuffleMode::On`, `advance_track` **never** reaches the loop-off stop path: an empty deck is rebuilt and playback continues forever. Advertised `Loop: Off` is a no-op under shuffle.

`previous_track` ignores the deck and walks `active_playlist_index - 1` linearly.

Replace the PRNG with a stored xorshift (no new crate):

```rust
struct XorShift64(u64);

impl XorShift64 {
    fn from_entropy() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);
        Self(nanos | 1)
    }
    fn next_bound(&mut self, max: usize) -> usize {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x as u64 as usize) % max.max(1)
    }
}
```

Keep it on `AppState`, seed once in `new`. In Fisher-Yates, `j = rng.next_bound(i + 1)`.

Shuffle + EOF:

```rust
if self.playback.shuffle_mode == ShuffleMode::On {
    if self.shuffle_deck.is_empty() {
        if is_auto_eof && self.playback.loop_mode != LoopMode::All {
            self.playback.is_playing = false;
            self.playback.is_paused = false;
            self.playback.current_time_sec = 0.0;
            return;
        }
        self.reset_shuffle_deck();
    }
    if let Some(next_idx) = self.shuffle_deck.pop() {
        self.play_playlist_index(next_idx);
        return;
    }
}
```

For previous under shuffle, push the current index back onto the deck (or keep a `shuffle_history: Vec<usize>`) instead of `current_idx - 1`.

---

### 1.7 Keymap drops keys and misses Shift-produced chars

Two TUI-correctness issues in `src/keymap.rs`.

**Prefix swallow.** `g` is a prefix of `gg`. `g` then `j`: not an exact match, not a prefix, `pending.clear()`, return `None`. The `j` is eaten. Standard fix: on mismatch, clear the prefix and **re-feed the last chord**.

**Modifiers.** Bindings all use `KeyModifiers::NONE`. Crossterm on many emulators reports `?` as `Char('?') + SHIFT`, `G` as `Char('G') + SHIFT`, `+` as `Char('+') + SHIFT`. Those lookups fail; Help / jump-to-bottom / volume-up are emulator-dependent. This is exactly the “robust across terminal emulators” question.

```rust
impl From<KeyEvent> for KeyChord {
    fn from(evt: KeyEvent) -> Self {
        let mut modifiers = evt.modifiers;
        if matches!(evt.code, KeyCode::Char(_)) {
            modifiers.remove(KeyModifiers::SHIFT);
        }
        Self { code: evt.code, modifiers }
    }
}

pub fn feed(&mut self, chord: KeyChord, keymap: &KeyMap) -> Option<Action> {
    if self.last_key_time.elapsed() > self.timeout {
        self.pending.clear();
    }
    self.last_key_time = Instant::now();
    self.pending.push(chord.clone());

    if let Some(action) = keymap.lookup(&self.pending) {
        self.pending.clear();
        return Some(action);
    }
    if keymap.is_prefix(&self.pending) {
        return None;
    }
    // mismatch: retry the last key as a fresh sequence
    self.pending.clear();
    if let Some(action) = keymap.lookup(std::slice::from_ref(&chord)) {
        return Some(action);
    }
    if keymap.is_prefix(std::slice::from_ref(&chord)) {
        self.pending.push(chord);
    }
    None
}
```

---

### 1.8 `play_track` clones a `Track` it already owns

Not a render-loop cost, but it is a one-line borrow-checker miss that every play path pays (`TogglePause` resume, playlist index, loop-track, folder start):

```diff
 pub fn play_track(&mut self, track: Track) {
-    self.playback.current_track = Some(track.clone());
     self.playback.is_playing = true;
     self.playback.is_paused = false;
     self.playback.current_time_sec = 0.0;
     self.playback.duration_sec = track.duration_sec;

     let _ = self.cmd_tx.send(MpvCommand::LoadFile {
         path: track.path.to_string_lossy().to_string(),
         replace: true,
     });
+    self.playback.current_track = Some(track);
 }
```

`enter_selected` clones the whole `BrowserEntry` to satisfy the borrow checker. Match by index, clone only the `PathBuf` / `Track` you need:

```rust
pub fn enter_selected(&mut self) {
    let Some(idx) = self.table_state.selected() else { return };
    match self.browser_items.get(idx) {
        Some(BrowserEntry::ParentDir(parent_path)) => {
            let parent_path = parent_path.clone();
            if resolve_in_jail(&self.music_root, &parent_path).is_some() {
                self.current_dir = parent_path;
                self.reload_current_directory();
                self.table_state.select(Some(0));
            }
        }
        Some(BrowserEntry::Directory { path, .. }) => {
            let Some(path) = resolve_in_jail(&self.music_root, path) else { return };
            self.current_dir = path;
            self.reload_current_directory();
            self.table_state.select(Some(0));
        }
        Some(BrowserEntry::AudioTrack(track)) => {
            let track = track.clone(); // unavoidable until Arc<Track>; see §3
            self.set_folder_as_active_playlist(&track);
        }
        None => {}
    }
}
```

---

## 2. Micro-optimizations (allocation / redundancy)

Line-level. Do these after §1; none of them save you if `time-pos` still rebuilds the table 50×/s.

| Location | What | Change |
|---|---|---|
| `browser.rs:61-62, 70` | `to_lowercase()` allocates two `String`s per comparison during sort | `a_name.to_ascii_lowercase()` still allocates. Prefer `a_name.cmp_ignore_ascii_case(b_name)` (Rust 1.89+) or a borrowed `unicase`-less ASCII compare: `a_name.as_bytes()` zip/lower. Music folders with non-ASCII names should use `unicode-case-mapping` only if you actually have them; default `eq_ignore_ascii_case` is enough for track `01` vs `02`. |
| `layout.rs:57, 71, 200-202, 225` | One `format!` per frame for header/player | Acceptable. If you want zero-alloc: keep `loop_label: &'static str` (already have `display_str()`) and use `Span::raw("[Loop: ")` + `Span::raw(mode.display_str())` + `Span::raw("]")` instead of building a `String`. Same for shuffle. Volume still needs a format; write into a `[u8; 16]` via `itoa` or `write!` on a cached `String` field on `PlaybackState`. |
| `layout.rs:195` | `format!("{} — {}", artist, title)` every player-bar frame | Cache `now_playing_label: String` on `play_track` / metadata patch. |
| `layout.rs:239-244` vs `track.rs:40-45` | Two identical `format_seconds` | One function. Player-bar gauge still needs a live format; reuse the helper, don’t duplicate. |
| `app.rs:75` | `canonical_root.clone()` then move original into `current_dir` | Fine. One PathBuf clone at startup. |
| `app.rs:337-341` | Clones every `Track` into `active_playlist` | Correct *lifetime* (playlist must survive leaving the folder). Expensive copies (PathBuf + 5 Strings each). See `Arc<Track>` in §3. Until then, `Vec<Track>` is OK — this is a user-gesture path, not per-frame. |
| `app.rs:317` | `track.path.clone()` in `locate_playing_track` | Split borrows: copy `PathBuf` for the parent, then search. Unavoidable without `Arc`. Cheap. |
| `protocol.rs:70` | `path.clone().into()` for JSON | `serde_json::Value::String(path.clone())` is required unless `LoadFile` stores `Value` directly. Fine. |
| `scanner.rs:34` | `batch.push(track.clone()); tracks.push(track)` | Dead code, but when you revive it: `tracks.push(track); batch.push(tracks.last().unwrap().clone())` is the same cost. Prefer sending patches by path, not cloning whole tracks twice. |
| `main.rs:96-106` | Initial `draw` then `should_render = true` draws again on first loop iteration | Set `should_render = false` after the initial draw, or drop the pre-loop draw. |
| `main.rs:200-204` | `dirs_home()` falls back to `/home/arcioth` | Portable fallback is `"/"`, not a single developer home. Target list is Arch/NixOS/Fedora/CachyOS with other users. |
| `Cargo.toml` | `tokio = { features = ["full"] }`, `ratatui = { features = ["all-widgets"] }`, unused `toml`, `futures` + `futures-util` both pulled, `chrono` only for a crash timestamp | Trim tokio to `rt`, `rt-multi-thread` or better `rt` + `macros` + `net` + `process` + `sync` + `time` + `fs` + `signal`. Drop `all-widgets`. Use `futures_util` only. Replace `chrono` RFC3339 with `SystemTime` debug format and you shed a crate. `toml` stays if config is next; otherwise drop it. This is how you keep the 4.5 MB claim as features grow. |
| `mpv.rs` Drop | `libc::kill(SIGTERM)` then `child.start_kill()` (SIGKILL) with no wait | SIGTERM is a no-op if SIGKILL is sent immediately. Either `start_kill()` only, or SIGTERM + short wait + SIGKILL. |
| `is_playing` / `is_paused` | Two booleans that are almost always inverses, then `Idle` sets both false | Keep a `enum Play { Stopped, Paused, Playing }` before Lua starts reading both flags and disagreeing with mpv. `handle_mpv_event("pause")` currently sets `is_playing = !paused`, which can mark Idle-with-no-file as Playing if a stray pause=false arrives. |
| `Track::is_audio_file` | `ext.to_lowercase()` allocates | `eq_ignore_ascii_case` against the list. |
| Unicode in the table | `🎵 📁 ⬆ ▶ ⏸` as column 0 with `Constraint::Length(5)` | Emoji width is 1 or 2 cells depending on emulator/font (kitty vs. xterm vs. foot vs. Windows Terminal). Hit-testing X is currently full-row so this does not break clicks, but the highlight symbol `"▶ "` plus a 2-cell emoji will eat the title column on some terms. Prefer ASCII (`>`, `D`, `M`, `#`) or test on foot + kitty before Lua draws custom views with the same glyphs. |

---

## 3. Architectural guardrails (before `mlua`)

`AppState` is a god object: every field `pub`, `handle_action` is `&mut self`, render takes `&mut AppState` so it can write `table_state` / hitboxes, `Action::Custom(String)` is a no-op, `KeyMap` is a static `HashMap` not owned by the app, the “window compositor” is a `bool show_help`. A Lua worker with `&mut AppState` or a cloned `AppState` will deadlock, tear, or skip the jail.

Do these **before** adding the `mlua` crate.

### 3.1 One mutation path: `Action` in, snapshots out

```
┌────────────┐     Action      ┌─────────────┐     MpvCommand     ┌──────────┐
│ keymap /   │ ──────────────► │  reducer    │ ─────────────────► │ mpv actor│
│ mouse /    │                 │  (main task)│ ◄── MpvEvent ───── │          │
│ Lua worker │ ──────────────► │             │                    └──────────┘
└────────────┘                 │  AppState   │
                               │  (not Sync) │
                               └──────┬──────┘
                                      │ UiSnapshot (owned, Send)
                                      ▼
                               render(&snapshot)  // no &mut AppState
```

- Lua **never** sees `AppState`. It sends `Action` (already `Serialize`) on an `mpsc`.
- Render takes `&AppState` plus a `TableState` that ratatui still needs `&mut` for. Pull `table_state` / hitboxes into `UiGeom` that only the UI task touches. Lua does not read `Rect`s.
- `handle_mpv_event` stays on the UI task. Lua observes playback through a `PlaybackSnapshot` sent at 4 Hz or on discrete transitions, not by sharing `PlaybackState`.

`Action::Custom(String)` is too wide. Replace with:

```rust
pub enum Action {
    // ...existing typed variants...
    Plugin {
        plugin_id: u32,
        name: Box<str>,
        payload: serde_json::Value,
    },
}
```

Unknown `name` is a no-op **and** a trace warning, never an error that kills the player.

### 3.2 Permission gating at the reducer, not in Lua

Lua 5.4 + `mlua` still has `load`, debug, and (if you enable them) `io` / `os`. Assume a plugin is hostile.

Minimum capability set, stored next to the plugin id, checked in `handle_action` when `action_source == Source::Plugin(id)`:

| cap | allows |
|---|---|
| `ui.overlay` | push/pop a floating window |
| `playback.control` | pause / seek / next / volume / loop / shuffle |
| `playback.queue` | `PlayTrackIndex`, replace playlist |
| `fs.jail_read` | list/read under `music_root` via `resolve_in_jail` |
| `fs.jail_write` | nothing by default; do not add until you have a real need |
| `keys.bind` | `KeyMap::bind_*` at runtime, namespaced, cannot override `Quit` |
| `net` | **deny**. Never `mlua` `io`/`os`/`package` |

Host-originated actions (keymap, mouse) skip the cap check.

Kill-switch: a plugin that panics, exceeds `set_memory_limit`, or runs longer than N ms in `spawn_blocking` is unloaded; the player keeps going. `mlua` instruction / memory limits belong on the registry, not as comments in a README.

### 3.3 Where Lua runs

- **Not** on the render/select loop.
- **Not** in the mpv actor (that thread must only speak JSON-IPC).
- `tokio::task::spawn_blocking` **or** a dedicated `std::thread` with its own `mpsc`. `mlua` is not async. Blocking the reactor is how you get “stuck seekbar + stuck keys”.
- One Lua state per plugin, not a shared global. Shared globals + `Send` is how you get aliasing across the cap table.

Do not let Lua call ratatui. Give it a **declarative view**: `View { kind: Table | Paragraph | Gauge, title, rows, ... }` that the compositor renders. Immediate-mode Lua calling `Frame` will hold `&mut Frame` across a yield you cannot see.

### 3.4 Compositor: a stack, not a bool

```rust
pub struct Window {
    pub id: WindowId,
    pub title: String,
    pub view: View,
    pub source: Source, // Host | Plugin(id)
}

pub windows: Vec<Window>; // last = top
```

Input routing: top window consumes keys/mouse (what `show_help` already does) until `CloseTopWindow`. `Quit` always works. Hit-test mouse against the top window’s rect first (the help-modal passthrough bug in §1.3 is the same hole Lua will widen).

`centered_rect` in `window.rs` is fine; keep it.

### 3.5 `Arc<Track>` before plugins touch the library

Today a play clones the whole folder (`set_folder_as_active_playlist`) plus `current_track`. Lua will want the same list. `Track` is `PathBuf` + five `String`s. Make it:

```rust
pub type TrackRef = Arc<Track>;
```

`browser_items`, `active_playlist`, `current_track` all hold `TrackRef`. Cloning a playlist is an Arc bump. Metadata patches (`ScannerEvent::Batch`) use `Arc::make_mut` or replace the Arc if you keep tracks immutable (prefer immutable: scan produces a new `Arc<Track>`, swap it in).

This is the clone-elimination that actually matters at folder scale. Per-frame clones in §1.2 matter at *frame* scale; this matters at *plugin* scale.

### 3.6 Keymap becomes data

`KeyMap` is already a `HashMap<Vec<KeyChord>, Action>`. Persist it (this is what `toml` in `Cargo.toml` is for). Lua `keys.bind` inserts into a **plugin layer** that is searched after host defaults, never before `Quit` / `CloseTopWindow`. Do not let Lua replace the state machine.

Normalize Shift as in §1.7 so plugin bindings for `?` / `+` / `G` work on kitty, foot, and xterm.

### 3.7 mpv actor contract for plugins

- Plugins do not get the UNIX socket path. They do not speak JSON-IPC.
- They send `Action` / `MpvCommand` is **not** public to Lua. Volume, seek, load go through the reducer so `PlaybackState` stays consistent (today `TogglePause` already optimistic-updates; Lua skipping that and sending `MpvCommand::SetPause` directly would desync the UI).
- Hide `cmd_tx` (`pub` today on `AppState`). The reducer is the only writer.

### 3.8 Tests that must exist before mlua lands

There are no tests at all. The Lua layer will encode the same invariants. Minimum, as `#[cfg(test)]` in the modules that already exist:

1. **Jail:** `resolve_in_jail(root, root.join("a/../../etc"))` is `None`. Symlink to `/tmp` is `None`. Child of root is `Some`.
2. **Shuffle:** deck of `n` is a permutation of `0..n` minus current; two resets with the same seed are independent only if the PRNG advances (seeded xorshift makes this deterministic — good).
3. **advance_track:** `LoopMode::Off` + last track + `is_auto_eof=true` stops; `LoopMode::All` wraps; `LoopMode::Track` reloads the same path; shuffle + `LoopMode::Off` at deck end stops.
4. **Keymap:** `g` then `j` yields `MoveDown(1)`; `Char('?')+SHIFT` yields `ToggleHelp`.
5. **Reducer isolation:** `Action::Custom` / unknown plugin name does not panic and does not touch playback.

Do not start from `ScannerEvent` integration tests until listing is async (§1.5).

### 3.9 What not to do

- Do not `Clone` `AppState` onto a Lua thread. `TableState` + `UnboundedSender` + path vecs will compile (`cmd_tx` is `Clone`) and then two reducers will fight mpv.
- Do not add `Mutex<AppState>` so Lua can “just lock”. The render loop would hitch on every plugin timer.
- Do not enable `mlua` `vendored` + `lua54` and also leave `loadfile` / `package.searchers` intact. Strip `io`, `os`, `debug`, `package`, `load`, `loadfile`, `dofile`. Expose a host `plugin.require(name)` that only loads from `$XDG_CONFIG_HOME/tunotron/plugins/<name>/`.
- Do not follow the unused scanner’s `follow_links(true)` when you give Lua a library API.

---

## Suggested order of work (for the implementer)

1. §1.1 time-pos / dirty flag / bounded channel / LinesCodec cap — biggest CPU win, no UX change.
2. §1.2 borrow strings in the table; cache `duration_label`.
3. §1.3 mouse rects from `Block::inner` + `TableState::offset`; clip help modal.
4. §1.4 `resolve_in_jail` + no dir-symlink follow.
5. §1.5 async metadata; wire `AppEvent::Scanner`.
6. §1.6 / §1.7 shuffle PRNG + loop/prev; keymap Shift + prefix retry.
7. §1.8 `play_track` move; `enter_selected` by index.
8. §2 table leftovers, crate trim, `Play` enum.
9. Tests in §3.8.
10. **Then** `Arc<Track>`, window stack, cap table, `mlua` behind a feature flag.

Do not add `mlua` in the same patch as 1–9. The reducer and jail need to be the API you are willing to freeze.
