> **WARNING: THIS PROJECT IS A PROOF OF CONCEPT AND HAS BEEN MOSTLY VIBE CODED. Treat it with all due suspicion. This project is unsupported.**

# MediaSandbox

A Unity Editor tool for drag-and-dropping media files (images, animations, audio) into play mode and seeing them rendered/played immediately. Decoding runs inside a WebAssembly sandbox so malformed or malicious files can't reach native code.

## What it does

- Opens a **Media Drop** editor window when you enter play mode
- Accepts dragged files and reads their type without trusting the file extension — format detection happens inside WASM
- Decodes the file off the main thread and spawns the result back on it:
  - **Images** (PNG, JPEG, BMP, TIFF, WebP, HDR, QOI) → textured Quad
  - **Animations** (GIF, animated WebP) → Quad with a coroutine-driven frame loop
  - **Audio** (MP3, FLAC, OGG/Vorbis, WAV, AIFF) → `AudioSource` that starts playing
  - **Video** → detected and logged; spawning not yet implemented
- Rejects files over 512 MB before decode; images larger than 8192 px in either dimension are scaled down to fit (full decode then resize — per-format fast-path is planned, see below)

## Getting Started

Requirements:

- Unity 2022.3 or later (tested on 2022.3 LTS)
- The `Wasmtime` NuGet package is vendored under `Assets/Packages/` — no additional install step
- `wasmtime.dll` (native runtime) is under `Assets/Plugins/`
- The compiled `decoder.wasm` lives at `Assets/StreamingAssets/mediasandbox/decoder.wasm`

To use:

1. Open the project in Unity
2. Enter play mode — the **Media Drop** window opens automatically
3. Drag a media file onto the window or the Game view
4. Check the Console and Scene for the spawned object

No setup component or scene configuration is required; `InitializeMediaSandbox` wires everything up via `[InitializeOnLoad]`.

## Building

The Unity C# side requires no build step beyond normal Unity compilation.

To rebuild `decoder.wasm` from source:

```powershell
pwsh scripts/build-wasm.ps1          # release-with-debuginfo (default)
pwsh scripts/build-wasm.ps1 -Release # smallest output
pwsh scripts/build-wasm.ps1 -Debug   # unoptimised
```

Output lands at `Assets/StreamingAssets/mediasandbox/decoder.wasm`. You need the Rust toolchain with the WASM target:

```
rustup target add wasm32-wasip1
```

To rebuild the native Wasmtime runtime for a specific platform, see `scripts/build-wasmtime-*.sh` / `.ps1`. The CI workflow (`.github/workflows/build.yml`) builds all platforms and uploads artifacts.

## Architecture

```
Unity Editor (C#)
  InitializeMediaSandbox      — [InitializeOnLoad], wires play-mode hooks
  MediaDropWindow             — EditorWindow that receives drag-drop events
  DragDropMediaSpawner        — reads file bytes, dispatches to sandbox, spawns Unity objects
  MediaDecoderSandbox         — owns the Wasmtime Engine/Linker/Module (shared); 
                                creates one Store+Instance per decode call for concurrency

WASM boundary (Wasmtime .NET SDK + wasmtime.dll)
  decoder.wasm                — compiled from decoder/ Rust crate
    query_attributes          — sniff format, return dimensions/duration/buffer sizes
    query_metadata            — → JSON with EXIF fields and raw XMP packet
    strip_metadata            — removes EXIF/XMP in-place (JPEG, PNG, WebP; no re-encode)
    decode_image              — → raw RGBA bytes
    decode_animation          — → frame count, per-frame delay + RGBA
    decode_audio              — → interleaved f32 PCM, sample rate, channel count
    encode_image              — → PNG or JPEG bytes (unused by spawner, available for export)

Rust crate (decoder/)
  image crate                 — PNG, JPEG, BMP, TIFF, WebP, HDR, QOI, GIF
  symphonia                   — MP3, FLAC, OGG/Vorbis, WAV, AIFF
```

Memory is managed explicitly: the host calls `alloc`/`dealloc` exports on the WASM instance. Each decode gets its own `Store`, so concurrent calls don't share state.

### Unsafe code

All `unsafe` in the Rust crate is at genuine FFI/allocator boundaries — there is no unsafe used for convenience. Every block falls into one of these categories:

- **FFI ptr+len → slice** (`from_raw_parts` / `from_raw_parts_mut`): every exported function receives raw pointer + length pairs from the C ABI. There is no safe way to construct a Rust slice from these.
- **Writing through raw output pointers** (`*(ptr as *mut u32) = value`): result values (frame count, delay, buffer addresses) are written back to host-allocated memory passed in as raw pointers.
- **Global allocator** (`std::alloc::alloc` / `dealloc`): the `alloc` and `dealloc` WASM exports call the Rust global allocator directly, which is intrinsically unsafe.
- **`ManuallyDrop::drop` in `AnimHandle`**: drop order must be explicit — the `Frames` iterator is torn down before the backing allocation it borrows from is freed. Rust's automatic drop order cannot express this, so `ManuallyDrop` and an explicit `Drop` impl are required.
- **`Box::into_raw` / `Box::from_raw`** in `animation_open` / `animation_close`: the streaming decoder handle is a type-erased `u32` passed across the FFI boundary; boxing and unboxing it requires unsafe.

## Scope

This is a sandbox tool for inspecting and previewing media inside the Unity Editor. It is not intended for shipping in a player build, production asset pipelines, or any context requiring stability guarantees. The WASM boundary provides a degree of isolation from malformed files but has not been audited for security.

Notable gaps:

- Video spawning is stubbed (logs a message, does nothing)
- No UI for configuring `SandboxLimits` at runtime
- Texture upload, AudioClip creation, and quad spawning are minimal/unpolished
- Error handling surfaces to the Console only — no in-editor UI feedback

## Future Work

Items that are known, understood, and explicitly deferred:

**Decode performance**
- *Per-format target-sized decode* — large images are decoded at full resolution then scaled down. `zune-jpeg` uses 1/8, 1/4, 1/2 IDCT variants internally but does not expose a scale-factor in its 0.5 public API; PNG filter dependencies require every row regardless. Both remain full-resolution for now. See `decoder/src/img.rs`.

**EXIF / metadata**
- *TIFF metadata stripping* — `strip_metadata` handles JPEG, PNG, and WebP in-place. TIFF's IFD structure is tightly interwoven with the image data, making segment-level removal impractical without a dedicated library; use the decode→`encode_image` round-trip for TIFF when a re-encode is acceptable.

**Runtime**
- *AOT-compiled module* — Wasmtime supports ahead-of-time compilation via `Engine.PrecompileModule()` → `Module.Deserialize()`. Shipping a pre-compiled `.cwasm` file would eliminate the Cranelift JIT stall on first load (desktop) and the Pulley interpreter overhead on platforms where JIT is prohibited (iOS).

  Platform matrix once implemented:
  - iOS: AOT `.cwasm` (preferred) or Pulley interpreter fallback
  - Desktop/Android: JIT Cranelift (current default), AOT `.cwasm`, or Pulley interpreter

  The `UsePulley` const in `MediaDecoderSandbox` would expand to a three-way `enum ExecutionMode { Jit, Aot, Interpreter }`, and `Awake()` would probe for a `.cwasm` sidecar before falling back. AOT compilation would be a build-script step in `build-wasm.ps1`. Security consideration: a stale AOT module that predates a patched WASM binary needs an explicit policy for when to prefer fresh interpretation over a cached `.cwasm`.

**Format support**
- *PSD* — flatten or expose layers. Per-layer blend modes, masks, and 16/32 bpc channels are non-trivial. Flattened RGBA is the natural starting point.
- *PDF* — vector-to-raster at a caller-supplied DPI, multi-page navigation (the `page_count` field in `AttrResult` is already reserved), CMYK colorspace conversion. `pdfium-render` (Google PDFium) is the highest-fidelity option; the tradeoff is ~10–20 MB added to the WASM binary. The streaming per-page interface is designed: `pdf_open` / `pdf_page_size` / `pdf_render_page` / `pdf_close`, mirroring the animation streaming API, with a C# `PdfDocument` wrapper.

## License

MIT
