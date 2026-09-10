# 🧠 Grok Optimization & Architecture Review Instructions

This document contains instructions and the exact prompt to feed into **Grok** to perform a deep optimization and code-quality audit of the **Tunotron** codebase before we build the Lua extension engine.

---

## 📋 Instructions for the User

1. **How to run the review:**
   - If using the **Grok CLI**:
     ```bash
     cd ~/Documents/tunotron
     # Pass the prompt and relevant source files to grok, piping the result to GROK_REVIEW.md
     grok "Review the attached project for optimizations" --file GROK_REVIEW_PROMPT.md --file src/main.rs --file src/app.rs --file src/audio/mpv.rs --file src/library/browser.rs --file src/ui/layout.rs > GROK_REVIEW.md
     ```
   - Or if copying into **Grok Web / Chat**:
     - Copy the prompt in **Section 2** below.
     - Attach or paste `src/main.rs`, `src/app.rs`, `src/audio/mpv.rs`, `src/library/browser.rs`, and `src/ui/layout.rs`.
     - Instruct Grok to save or output its response.
     - Save the final response directly to:
       ```
       ~/Documents/tunotron/GROK_REVIEW.md
       ```

2. **Output Destination:**
   - **File path:** `~/Documents/tunotron/GROK_REVIEW.md`
   - As soon as that file is written, Antigravity will detect it, read its recommendations, and implement the high-value optimizations.

---

## 🎯 The Review Prompt for Grok

*(Copy everything below this line into Grok)*

```markdown
# Role: Principal Systems & Rust Performance Architect
You are reviewing "Tunotron", a high-performance, minimal terminal music player written in Rust (Ratatui, Crossterm, Tokio) backed by a headless `mpv` process communicating via UNIX domain socket JSON-RPC.

## Architectural Context:
- Target platforms: Arch Linux, NixOS, Fedora, CachyOS.
- Current binary size: 4.5 MB release build. Zero runtime dependencies other than system libc and mpv.
- Design:
  1. The folder IS the active playlist (linear playback, Fisher-Yates deck shuffle, robust pause/resume).
  2. Jailed filesystem security: users cannot navigate higher than the starting music root directory.
  3. Zero CPU usage when idle via a reactive `tokio::select!` event loop and guarded tick timers.
  4. Floating window compositor (currently hosting a Help modal, soon to host dynamic Lua extension views).
  5. Next upcoming phase: A sandboxed Lua 5.4 extension engine (`mlua`) with strict permission gating.

## Review Scope:
Review the codebase with a sharp eye for:

### 1. Memory Allocations & Hot-Path Efficiency
- Identify any redundant `.clone()` calls in `src/app.rs` and `src/ui/layout.rs` (especially during track switching, directory browsing, or inside the render loop).
- Can string formatting in `render_browser_table` or row generation be made zero-allocation or pooled?

### 2. Async I/O & IPC Safety (`src/audio/mpv.rs`)
- Review the `MpvSupervisor` (using Linux `PR_SET_PDEATHSIG` in `pre_exec`). Are there any race conditions or signal safety concerns?
- Review the `FramedRead` / `LinesCodec` event loop. Is backpressure handled properly if mpv emits rapid property changes?

### 3. TUI Rendering & State Reducer Bottlenecks (`src/main.rs` & `src/ui/layout.rs`)
- Is the dirty-flag batch coalescing in `main.rs` optimal?
- Are terminal resize and mouse hitbox containment checks (`browser_rect`, `progress_rect`) robust across different terminal emulators?

### 4. Architectural Readiness for the Sandboxed Lua Layer
- Is `AppState` and `Action` sufficiently decoupled so that a background Lua worker thread communicating over `mpsc` channels can drive UI views, playback, and keybindings without lock contention?

## Output Requirement:
Provide your review formatted in markdown. Include:
1. **Critical Optimizations (High Impact / Low Risk)**: Concrete code snippets with before/after diffs.
2. **Micro-Optimizations (Allocation / Redundancy)**: Specific line-by-line efficiency fixes.
3. **Architectural Guardrails**: Specific suggestions before we implement the `mlua` sandbox.
Please write your entire output directly so it can be saved to `GROK_REVIEW.md`.
```
