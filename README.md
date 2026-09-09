# 🎵 Tunotron

A minimal, rock-solid, extensible terminal music player built with Rust, Ratatui, and headless `mpv`.

Tunotron is built on the philosophy of **"Linear, zero-friction playback on a bulletproof base"**:
- **The folder IS the playlist:** Open any folder of songs, press Enter, and the entire album plays linearly, looped, or shuffled.
- **Jailed Filesystem Security:** Strict boundary at the specified music root. The player and extensions cannot navigate into root or dotfile systems.
- **Flawless pause and resume:** Toggling pause with Space resumes seamlessly from the exact millisecond—no restarts.
- **True Fisher-Yates Shuffle:** Non-repeating randomized shuffle deck that never repeats until the full folder is played.
- **Locate Song:** Press `.` to immediately snap the cursor and folder view back to the currently playing song.
- **Full Mouse Support:** Scroll with mouse wheel, click to select tracks, click the progress bar to seek.
- **Instant startup** (<5ms) and tiny memory footprint (<10MB, single 4.5MB binary).
- **0.0% CPU usage when idle** via an asynchronous reactor event loop and guarded tickers.
- **Window Compositor Layer:** Floating modals (like the built-in `?` Help screen) render cleanly above the main interface.

---

## 🎹 Controls & Shortcuts

### Navigation (Keyboard & Mouse)
| Key / Input | Action |
| :--- | :--- |
| `j` / `↓` / **Mouse Scroll Down** | Move down (scroll) |
| `k` / `↑` / **Mouse Scroll Up** | Move up (scroll) |
| **Mouse Left Click (Track)** | Select track |
| `g g` | Jump to top of directory |
| `G` | Jump to bottom of directory |
| `.` | **Locate currently playing track** (jumps to its folder and highlights it) |
| `d` / `u` | Half-page down / up |
| `<Enter>` | Open folder or play selected track |
| `<Backspace>` | Go up to parent directory (jailed to music root) |

### Playback Controls
| Key / Input | Action |
| :--- | :--- |
| `<Space>` / `c` | Toggle Play / Pause (resumes seamlessly) |
| `b` | Next track in folder playlist |
| `z` | Previous track (or restart track if >3s) |
| `v` | Stop playback |
| `m` | Cycle Loop Mode (`Off` → `Track` → `All`) |
| `s` | Toggle Shuffle Mode (`Off` ↔ `On`, non-repeating deck) |
| `→` / `←` | Seek forward / backward 5 seconds |
| **Mouse Left Click (Progress Bar)** | **Seek directly to clicked position** |
| `+` / `=` | Increase volume |
| `-` | Decrease volume |

### View & Modals
| Key | Action |
| :--- | :--- |
| `Z` | Toggle View Density (`Comfortable` ↔ `Compact`) |
| `r` | Reload current directory from disk |
| `?` | Toggle Help modal window |
| `<Esc>` | Close active modal window |
| `q` | Quit cleanly (restores terminal & terminates mpv daemon) |

---

## 🛠️ Building & Running

### Run on NixOS
```bash
cd ~/Documents/tunotron
nix-shell --run "cargo run --release -- ~/Music"
```

### Run on Arch Linux / Fedora / Ubuntu
```bash
cd ~/Documents/tunotron
cargo run --release -- ~/Music
```
