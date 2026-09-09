# 🎵 Tunotron

A minimal, rock-solid, extensible terminal music player built with Rust, Ratatui, and headless `mpv`.

Tunotron is built on the philosophy of **"Linear, zero-friction playback on a bulletproof base"**:
- **The folder IS the playlist:** Open any folder of songs, press Enter, and the entire album plays linearly, looped, or shuffled.
- **Flawless pause and resume:** Toggling pause with Space resumes seamlessly from the exact millisecond—no restarts.
- **Loop and Shuffle modes:** Support for Loop All, Loop Track, and Shuffle out of the box (`m` and `s` keys).
- **Instant startup** (<5ms) and tiny memory footprint (<10MB, single 4.5MB binary).
- **0.0% CPU usage when idle** via an asynchronous reactor event loop and guarded tickers.
- **Window Compositor Layer:** Floating modals (like the built-in `?` Help screen) render cleanly above the main interface.
- **Decoupled headless `mpv` supervisor** using UNIX domain socket IPC (`JSON-RPC`) and Linux `PR_SET_PDEATHSIG` to guarantee zero zombie background processes.

---

## 🎹 Default Keybindings

### Navigation (Vim motions)
| Key | Action |
| :--- | :--- |
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `g g` | Jump to top of directory |
| `G` | Jump to bottom of directory |
| `d` / `u` | Half-page down / up |
| `<Enter>` | Open folder or play selected track |
| `<Backspace>` | Go up to parent directory |

### Playback Controls
| Key | Action |
| :--- | :--- |
| `<Space>` / `c` | Toggle Play / Pause (resumes seamlessly) |
| `b` | Next track in folder playlist |
| `z` | Previous track (or restart track if >3s) |
| `v` | Stop playback |
| `m` | Cycle Loop Mode (`Off` → `Track` → `All`) |
| `s` | Toggle Shuffle Mode (`Off` ↔ `On`) |
| `→` / `←` | Seek forward / backward 5 seconds |
| `+` / `=` | Increase volume |
| `-` | Decrease volume |

### Directory & Modals
| Key | Action |
| :--- | :--- |
| `r` | Reload current directory from disk (picks up added/removed songs) |
| `?` | Toggle Help modal window |
| `<Esc>` | Close active modal window |
| `q` | Quit cleanly (restores terminal & terminates mpv daemon) |

---

## 🛠️ Building & Running

### Prerequisites
- `mpv` (installed via system package manager)
- `cargo` and `rustc` (Rust 1.75+)

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
