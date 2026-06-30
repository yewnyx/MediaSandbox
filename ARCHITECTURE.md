# MediaSandbox — Architecture

Technical reference and design rationale for the WASM-sandboxed media decoder.

---

## Component breakdown

### Package: `xyz.yewnyx.MediaSandbox` (`unity_package/`)

The package delivers the native Wasmtime runtime for all supported platforms (`unity_package/Plugins/`) and the managed type library (`unity_package/Runtime/`).

**`MediaTypes`** contains the public API types that flow across the WASM boundary: `MediaType`, `ImageFormat`, `MediaAttributes`, `RawImageData`, `AnimationFrame`, `AnimatedImageData`, `RawAudioData`, and `PathologicalMediaException`.

**`SandboxLayout`** is a generated class containing exact field offsets and enum discriminants derived from the Rust types — see [C#↔Rust layout sync](#crust-layout-sync).

### Editor sandbox example (`Assets/MediaSandbox/` — not part of the package)

The repository includes a full working reference implementation. Consumers copy or adapt as needed; none of this ships with the package.

**`MediaDecoderSandbox`** is the only class that touches Wasmtime directly. It owns the `Engine`, `Linker`, and `Module` — created once in `Awake()` and shared for the lifetime of play mode. Every public method creates its own `Store` and `Instance` on the thread pool and disposes them when the call returns.

**`SandboxLimits`** holds project-specific policy constants: maximum file size before rejection and maximum image dimension before scaling. Consumers set their own limits.

**`InitializeMediaSandbox`** runs at editor load via `[InitializeOnLoad]` and hooks `EditorApplication.playModeStateChanged` to create the `[SYSTEM] MediaSandbox` GameObject on play entry. Nothing in the scene needs to be configured — it wires itself up.

**`MediaDropWindow`** is the `EditorWindow` that receives drag-drop events. It reads the raw file bytes from disk and hands them to `DragDropMediaSpawner`.

**`DragDropMediaSpawner`** orchestrates the full decode pipeline: calls `QueryAttributesAsync` to identify the format and get buffer sizes, dispatches to the appropriate decode method, then marshals the result back to the main thread to create a `Texture2D`, `AudioClip`, or animation coroutine.

### WASM side (Rust)

The `decoder/` crate compiles to `decoder.wasm`. Exported functions use a C ABI with `u32` pointer/length pairs rather than WASM multi-value returns, keeping the interface simple and easy to call from any host. The crate also builds as an `rlib` for the `gen_cs` host binary; see [Layout sync](#crust-layout-sync) below.

All decode logic delegates to established Rust libraries — `image` for raster formats, `symphonia` for audio, `kamadak-exif` for EXIF, `zune-jpeg` for cheap JPEG header/XMP extraction. The crate itself is thin plumbing: FFI shims, an allocator bridge, and format dispatch.

---

## Threading model

`Engine`, `Linker`, and `Module` are thread-safe by Wasmtime's contract — they are immutable after construction and internally reference-counted. `Store` and `Instance` are not thread-safe and are never shared; they are created at the start of each `Task.Run` lambda and dropped when it returns.

The result is genuine concurrency with no application-level locking. Concurrent decode calls produce independent WASM instances that share nothing: each has its own linear memory, its own allocator heap, and its own Rust stack. The .NET thread pool limits the number of instances alive simultaneously to roughly `Environment.ProcessorCount`, so memory footprint scales with active parallelism rather than queued task count.

---

## Memory management protocol

The WASM module exports `alloc(size: u32) -> u32` and `dealloc(ptr: u32, size: u32)`, backed by Rust's global allocator inside the instance. All allocations use 8-byte alignment.

**Input data:** the host calls `alloc` to reserve space inside WASM linear memory, copies the file bytes in via `Memory.GetSpan`, then passes the pointer and length to the decode function. After the call returns the host calls `dealloc`.

**Output data:** WASM allocates output buffers internally and writes back the pointer and length to host-provided scratch slots (small `alloc(4)` regions). The host reads the pointer and length, copies the bytes out, then calls `dealloc` on the WASM allocation.

This protocol means the host never needs to guess output sizes for fixed-shape outputs (frame buffers, encoded images) — it either pre-allocates the exact size from `AttrResult`, or reads the WASM-written length after the fact.

**`AttrResult` struct size:** the struct has 2 `u64` fields followed by 9 `u32` fields, totalling 52 bytes of data. `#[repr(C)]` requires the struct size to be a multiple of its largest field alignment (8), so 4 bytes of tail-padding are added — `size_of::<AttrResult>() == 56`. The host allocates 56 bytes for it, not 52.

---

## Two-phase decode

Every decode goes through `query_attributes` first. This reads only the file header — no pixels, no samples — and returns format, dimensions, frame count, sample rate, and the exact byte count of the output buffer the caller should allocate. The caller then allocates that buffer and passes it to the appropriate `decode_*` function.

The two-phase design has two concrete benefits. First, it avoids any guessing about buffer sizes and eliminates internal reallocations inside the hot decode path. Second, it gives callers the opportunity to apply policy before spending decode time — `DragDropMediaSpawner` uses `AttrResult` to reject pathologically large files and cap image dimensions before issuing the full decode.

---

## Streaming animation

Animations are decoded frame-by-frame via `animation_open` / `animation_next_frame` / `animation_close` rather than all at once. `animation_open` returns a handle (a `Box<AnimHandle>` cast to `u32` across the FFI boundary), `animation_next_frame` decodes one frame into a host-pre-allocated RGBA buffer and writes the frame delay in milliseconds, and `animation_close` frees the handle.

Ownership of the input buffer transfers to `animation_open` — the module takes its own copy and frees the caller's buffer internally on the same call. The host must not call `dealloc` on it afterward.

The streaming API lets the progress callback fire between frames, keeping the editor UI responsive during long GIF decodes.

---

## C#↔Rust layout sync

`AttrResult`'s field offsets and `MediaKind`'s integer discriminants are shared across the FFI boundary. Keeping them in sync by hand is fragile: a field added in Rust silently shifts every subsequent C# read offset with no compile error on either side.

`decoder/src/bin/gen_cs.rs` is a host Rust binary (not WASM) in the same crate. It uses `std::mem::offset_of!` and `std::mem::size_of!` against the actual Rust types and prints `unity_package/Runtime/Generated/SandboxLayout.g.cs` — a C# file with all layout constants as `const int` values. `build-wasm.ps1` runs this binary before every WASM build. The generated file is committed to the repository so Unity can compile it without requiring the build script to have run.

`MediaDecoderSandbox` calls `AssertLayoutSync()` from `Awake()` in Editor builds. This checks that the C# `MediaType` and `ImageFormat` enum values match `SandboxLayout.MediaKindValue` and `SandboxLayout.EncodeFormat`. If the Rust types are changed and the build script is not re-run, the assertion fires at play entry before any decode call runs.

`MediaKind` carries `#[repr(u32)]` with explicit discriminants so the generator can extract them via `MediaKind::Image as u32` rather than relying on implicit Rust enum ordering.

---

## Execution modes

Wasmtime offers three execution paths:

| Mode | How | Constraint |
|------|-----|-----------|
| Cranelift JIT | Compiles WASM → native machine code at load time | Requires marking memory pages executable; banned on iOS and some managed environments |
| Pulley interpreter | Portable WASM bytecode interpreter; no JIT | Runs everywhere; slower than JIT |
| AOT `.cwasm` | Pre-compiled ahead-of-time via `Engine.PrecompileModule()` | Not yet implemented; see Future Work in README |

**Pulley is the current default** (`UsePulley = true`). The .NET SDK does not expose the configuration option needed to select it, so `MakePulleyConfig()` walks `Config`'s private fields via reflection to locate the raw `wasm_config_t*` handle and calls `wasmtime_config_target_set("pulley64")` directly from the C API. On iOS, `#if UNITY_IOS` forces Pulley regardless of `UsePulley`.

JIT can be re-enabled by setting `UsePulley = false`, and was tested — Cranelift performs meaningfully better for decode-heavy workloads. Pulley is the default because it is the one path that works everywhere without per-platform conditionals.

The WASM C ABI is also an escape hatch: the same Rust function signatures could be compiled to a platform-native static library and called via P/Invoke, with no changes to the C# call sites beyond swapping the dispatch layer. On platforms where both the OS sandbox and memory-safe decoders provide sufficient isolation, this path eliminates WASM overhead entirely.

---

## Unsafe code

All `unsafe` in the Rust crate is at genuine FFI or allocator boundaries. Every block is one of:

- **FFI ptr+len → slice** (`from_raw_parts` / `from_raw_parts_mut`): exported functions receive pointer+length pairs from the C ABI. There is no safe way to construct a Rust slice from these.
- **Writing through raw output pointers** (`*(ptr as *mut u32) = value`): result values written back to host-provided scratch slots.
- **Global allocator** (`std::alloc::alloc` / `dealloc`): the `alloc` and `dealloc` WASM exports call the Rust global allocator directly.
- **`ManuallyDrop::drop` in `AnimHandle`**: the `Frames` iterator must be dropped before the allocation it borrows from is freed. Rust's automatic drop order cannot express this constraint.
- **`Box::into_raw` / `Box::from_raw`** in `animation_open` / `animation_close`: the streaming decoder handle is a type-erased `u32` across the FFI boundary.

---

## Design decisions (Q&A)

### Why WASM instead of native decoders?

Several reasons that compound:

**CVE burden.** Tracking every vulnerability in every embedded decoder library — and shipping a rebuild every time one is patched — is an ongoing, open-ended maintenance commitment. Sandboxing is the line of defense you fall back on when a vulnerability you haven't patched yet is triggered. Browsers do not embed image decoders raw; they sandbox them.

**Memory isolation.** WASM linear memory is the instance's entire address space. If the decoder panics, overflows a buffer, or is tricked into writing arbitrary bytes, those bytes stay inside the instance — they cannot reach the Unity Editor's heap, the GPU driver, or the OS. Memory-safe Rust reduces the *probability* of this happening; the WASM boundary limits the *blast radius* when it does. These are complementary, not redundant: Rust eliminates memory corruption bugs, but a logic bug that produces convincing-looking bad output is not caught by either.

**Updateability.** `decoder.wasm` can be replaced without rebuilding the Unity project. New formats, patched decoders, or extended functionality can ship out-of-band.

**Thread safety without per-library auditing.** Whether or not a given decoder library uses thread-local storage or has undocumented global state, the WASM instance boundary makes the question irrelevant. Each call gets its own instance; nothing is shared.

### Why not process-level sandboxing (IPC)?

Isolating the decoder in a separate process with IPC is the strongest available sandbox — it's roughly what browsers do for renderer processes — but it comes with costs that make it impractical here.

Platform support is the biggest one. iOS does not expose general process spawning outside of specific OS-managed infrastructure. Android requires `BackgroundService` and `Parcelable` plumbing with a service declaration in the manifest. Launching a subprocess inside a Unity Editor plugin, keeping it alive across domain reloads, and implementing a cross-process protocol for large binary payloads is a substantial, platform-specific engineering problem.

On the performance side, the argument that IPC is cheaper than a WASM interpreter doesn't hold clearly. A context switch to a child process evicts the CPU cache, which is expensive in proportion to how cache-sensitive the workload is. A JIT-compiled WASM call — which is what you'd use outside of iOS — has modest overhead and shares the cache with the calling thread. Even the Pulley interpreter competes favourably once you factor in the serialization cost of passing potentially large file buffers through a pipe or shared memory region.

### Isn't a slower interpreter a problem in a VR context?

VR is sensitive to *frame* latency — the time between when the headset samples head pose and when the corresponding frame reaches the display. Content decode doesn't touch that path. It runs entirely on the thread pool, off the main and render threads. A decoder running slower on a background thread is strictly better than a faster decoder that competes with the render loop for time.

At the extreme, heavy load causes background tasks to queue. This means a texture appears after more frames have elapsed, not that any frame is late. Nobody is nauseated because a texture took two extra frames to appear.

JIT can be re-enabled for platforms that allow it (`UsePulley = false`). It was tested and performs meaningfully better. Pulley is the default only because it is the one path that covers all platforms without conditionals.

### The decoders are already written in memory-safe Rust. Isn't the WASM sandbox redundant?

Memory safety and sandbox isolation are different properties. Rust's ownership system prevents memory corruption bugs — use-after-free, buffer overflows, data races. It does not prevent logic bugs: a decoder that misparses a malicious file and produces plausible-looking output, a format parser that enters an infinite loop, or a panic that would otherwise terminate the process. WASM's isolated linear memory catches the cases Rust doesn't: if something goes wrong in a way that produces arbitrary writes, those writes can't leave the instance. The Pulley interpreter also converts a WASM panic into a trapped exception that the .NET host catches and surfaces as a C# exception — the process keeps running.

The combination is stronger than either alone: Rust makes exploitation very difficult to construct, and WASM limits what a successful exploit achieves.

### Why not just turn JIT on everywhere and take the performance win?

On iOS, JIT requires marking pages writable-then-executable, which the OS prohibits for third-party applications outside of specific entitlements that are not available on the App Store. On some managed sandboxes (consoles, certain containerized environments), the same restriction applies. Making Cranelift the default would require per-platform logic and would either silently fall back or hard-fail on those platforms.

Pulley as the default means the same build works everywhere with no conditionals. Developers who are not shipping to iOS and want the performance headroom can flip `UsePulley = false`.
