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
    decode_image              — → raw RGBA bytes
    decode_animation          — → frame count, per-frame delay + RGBA
    decode_audio              — → interleaved f32 PCM, sample rate, channel count
    encode_image              — → PNG or JPEG bytes (unused by spawner, available for export)

Rust crate (decoder/)
  image crate                 — PNG, JPEG, BMP, TIFF, WebP, HDR, QOI, GIF
  symphonia                   — MP3, FLAC, OGG/Vorbis, WAV, AIFF
```

Memory is managed explicitly: the host calls `alloc`/`dealloc` exports on the WASM instance. Each decode gets its own `Store`, so concurrent calls don't share state.

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
- *Per-format fast-path resize* — large images are currently decoded at full resolution then scaled down. JPEG supports DCT-scale hints (1/2, 1/4, 1/8 natively); PNG can be downsampled scanline-by-scanline. Each format needs its own path to avoid the full-resolution intermediate. See `decoder/src/img.rs`.

**Color and format fidelity**
- *HDR output* — `.hdr` (Radiance) files are currently tone-mapped to 8 bpc on the Rust side. A proper path would output `f32` RGBA and create a `TextureFormat.RGBAFloat` texture. Relevant in linear-space VR projects where HDR is used for environment/lighting.
- *ICC color profiles* — the `image` crate ignores embedded ICC profiles. Most content is sRGB and will look correct; AdobeRGB or DCI-P3 tagged files will appear slightly desaturated. Full support requires `lcms2` bindings.
- *16/32 bpc decode* — PNG and TIFF can carry 16-bit channels. Currently downsampled to 8 bpc. Needed for print-originated assets.

**EXIF / metadata**
- *EXIF stripping for network transmission* — when transmitting the original compressed file bytes over a network, EXIF metadata (including GPS location) should be stripped. The current decode→`encode_image` round-trip already produces a metadata-free file; a dedicated strip-without-recompression function is the missing piece.

**Format support**
- *PSD* — flatten or expose layers. Per-layer blend modes, masks, and 16/32 bpc channels are non-trivial. Flattened RGBA is the natural starting point.
- *PDF* — vector-to-raster at a caller-supplied DPI, multi-page navigation (the `page_count` field in `AttrResult` is already reserved), CMYK colorspace conversion.

## License

MIT
