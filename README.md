# 🎵 Tunotron

A minimal, rock-solid, extensible terminal music player built with Rust, Ratatui, and headless `mpv`.

Tunotron is designed with the philosophy of **"Lego Modularity on a Bulletproof Base"**:
- **Instant startup** (<5ms) and tiny memory footprint (<10MB).
- **0.0% CPU usage when idle** via an asynchronous reactor event loop and guarded tickers.
- **Dedicated cmus-style views** (`1`: Library, `2`: File Browser, `3`: Queue) with isolated cursor/navigation states.
- **Decoupled headless `mpv` supervisor** using UNIX domain socket IPC (`JSON-RPC`) and Linux `PR_SET_PDEATHSIG` to guarantee zero zombie background processes.
- **Background library scanning** using `lofty` for tag extraction without dropping a single UI frame.

---

## 🎹 Default Keybindings

### Navigation (Vim motions)
| Key | Action |
| :--- | :--- |
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `g g` | Jump to top |
| `G` | Jump to bottom |
| `d` / `u` | Half-page down / up |

### Views (cmus model)
| Key | View |
| :--- | :--- |
| `1` | **Library View** (scanned audio tracks with ID3 metadata) |
| `2` | **File Browser** (direct filesystem directory navigation) |
| `3` | **Play Queue** (active playlist queue) |

### Playback Controls
| Key | Action |
| :--- | :--- |
| `<Enter>` | Play selected track or enter directory |
| `<Space>` / `c` | Toggle Play / Pause |
| `v` | Stop playback |
| `b` | Next track |
| `z` | Previous track |
| `a` | Enqueue selected track to Queue |
| `l` / `→` | Seek forward 5 seconds |
| `h` / `←` | Seek backward 5 seconds |
| `+` / `=` | Increase volume |
| `-` | Decrease volume |

### Application
| Key | Action |
| :--- | :--- |
| `r` | Rescan music directory |
| `q` | Quit gracefully (cleans up mpv daemon and restores terminal) |

---

## 🛠️ Building & Running

### Prerequisites
- `mpv` (installed via your system package manager)
- `cargo` and `rustc` (Rust 1.75+)

### Run on NixOS
Using the included `shell.nix`:
```bash
nix-shell --run "cargo run -- ~/Music"
```

### Run on Arch Linux / Fedora / Ubuntu
```bash
cargo run --release -- ~/Music
```
