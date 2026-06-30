> **WARNING: THIS PROJECT IS A PROOF OF CONCEPT AND HAS BEEN MOSTLY VIBE CODED. Treat it with all due suspicion. This project is unsupported.**

# MediaSandbox

A Unity package for decoding images, animations, and audio inside a WebAssembly sandbox so malformed or malicious files can't reach native code. Each decode call runs in its own Wasmtime instance with isolated linear memory; a compromised decoder can't escape it. See [ARCHITECTURE.md](ARCHITECTURE.md) for the full design rationale and technical details.

This repository also includes an editor sandbox (`Assets/MediaSandbox/`) that demonstrates the package: drag media files into play mode and see them rendered or played immediately.

## What it does

The package exposes an async C# API for decoding media inside an isolated WASM sandbox:

- **Format sniffing** — `QueryAttributesAsync` reads only the file header to identify type, dimensions, frame count, and exact output buffer size without trusting the file extension
- **Image decode** — `DecodeImageAsync` → raw RGBA bytes; supports PNG, JPEG, BMP, TIFF, WebP, HDR, QOI
- **Animation decode** — `DecodeAnimationAsync` → per-frame RGBA + delay; streaming, with progress callbacks; supports GIF and animated WebP
- **Audio decode** — `DecodeAudioAsync` → interleaved f32 PCM with sample rate and channel count; supports MP3, FLAC, OGG/Vorbis, WAV, AIFF
- **Metadata** — `QueryMetadataAsync` → EXIF fields and raw XMP packet as JSON; `StripMetadataAsync` removes metadata in-place from JPEG, PNG, and WebP without re-encoding
- **Encode** — `EncodeImageAsync` → PNG or JPEG bytes from raw RGBA

The bundled editor sandbox (`Assets/MediaSandbox/`) demonstrates all of the above: drag files into play mode and see them rendered or played immediately.

## Installing the Package

Add the package to your project's `Packages/manifest.json` via Git URL:

```json
{
  "dependencies": {
    "xyz.yewnyx.mediasandbox": "https://github.com/yewnyx/mediasandbox.git?path=/unity_package"
  }
}
```

Or open **Window → Package Manager → + → Add package from git URL** and paste the same URL.

## Getting Started

Requirements:

- Unity 2022.3 or later (tested on 2022.3 LTS)
- [Wasmtime .NET SDK](https://www.nuget.org/packages/Wasmtime) NuGet package — install via [NuGetForUnity](https://github.com/GlitchEnzo/NuGetForUnity) or place the DLL in `Assets/Plugins/`. The native Wasmtime runtime for all platforms is bundled with the package; only the managed SDK is needed separately.

Setup:

1. [Install the package](#installing-the-package)
2. Download `decoder.wasm` from the [latest CI build](../../actions/workflows/build.yml) (artifact `decoder-wasm`) and place it at `Assets/StreamingAssets/mediasandbox/decoder.wasm`
3. Copy `Assets/MediaSandbox/MediaDecoderSandbox.cs` from this repository into your project. This class is the reference implementation — it is not included in the package itself. Attach it to a persistent GameObject.
4. Call the async APIs on that component from your own scripts: `QueryAttributesAsync`, `DecodeImageAsync`, `DecodeAnimationAsync`, `DecodeAudioAsync`

**To run the bundled editor sandbox**: clone this repository, open it as a Unity project, and enter play mode.

## Building

The Unity C# side requires no build step beyond normal Unity compilation.

To rebuild `decoder.wasm` from source:

```powershell
pwsh scripts/build-wasm.ps1          # release-with-debuginfo (default)
pwsh scripts/build-wasm.ps1 -Release # smallest output
pwsh scripts/build-wasm.ps1 -Debug   # unoptimised
```

The build script also runs `gen_cs` (a host Rust binary in the same crate) to regenerate `unity_package/Runtime/Generated/SandboxLayout.g.cs` — a C# file containing exact struct offsets and enum discriminants derived from the Rust types. Commit it alongside the WASM binary.

Output lands at `Assets/StreamingAssets/mediasandbox/decoder.wasm`. You need the Rust toolchain with the WASM target:

```
rustup target add wasm32-wasip1
```

To rebuild the native Wasmtime runtime for a specific platform, see `scripts/build-wasmtime-*.sh` / `.ps1`. The CI workflow (`.github/workflows/build.yml`) builds all platforms and uploads artifacts.

## Architecture

```
Package: xyz.yewnyx.MediaSandbox (unity_package/Runtime/)
  MediaTypes                  — public API types: MediaAttributes, RawImageData, AnimatedImageData, RawAudioData, …
  SandboxLayout               — generated field offsets and enum discriminants (see Layout sync in ARCHITECTURE.md)

Editor sandbox example (Assets/MediaSandbox/ — not part of the package)
  MediaDecoderSandbox         — owns the Wasmtime Engine/Linker/Module (shared);
                                creates one Store+Instance per decode call for concurrency
  InitializeMediaSandbox      — [InitializeOnLoad], wires play-mode hooks
  MediaDropWindow             — EditorWindow that receives drag-drop events
  DragDropMediaSpawner        — reads file bytes, dispatches to sandbox, spawns Unity objects

WASM boundary (Wasmtime .NET SDK + wasmtime.dll)
  decoder.wasm                — compiled from decoder/ Rust crate
    query_attributes          — sniff format, return dimensions/duration/buffer sizes
    query_metadata            — → JSON with EXIF fields and raw XMP packet
    strip_metadata            — removes EXIF/XMP in-place (JPEG, PNG, WebP; no re-encode)
    decode_image              — → raw RGBA bytes
    animation_open/next_frame/close — streaming per-frame animation decode
    decode_audio              — → interleaved f32 PCM, sample rate, channel count
    encode_image              — → PNG or JPEG bytes (unused by spawner, available for export)
    alloc / dealloc           — WASM-side memory management, called by the host

Rust crate (decoder/)
  image crate                 — PNG, JPEG, BMP, TIFF, WebP, HDR, QOI, GIF
  symphonia                   — MP3, FLAC, OGG/Vorbis, WAV, AIFF
  kamadak-exif                — EXIF field extraction and JPEG orientation
  zune-jpeg                   — JPEG XMP extraction (header-only, no pixel decode)
```

Memory is managed explicitly: the host calls `alloc`/`dealloc` exports on the WASM instance. Each decode gets its own `Store` and `Instance` with isolated linear memory, so concurrent calls share nothing. See [ARCHITECTURE.md](ARCHITECTURE.md) for detail on threading, the memory protocol, layout sync, and the choice of WASM over alternatives.

## Scope

This is a sandbox tool for inspecting and previewing media inside the Unity Editor. It is not intended for shipping in a player build, production asset pipelines, or any context requiring stability guarantees. The WASM boundary provides a meaningful degree of isolation from malformed files but has not been audited for security.

Notable gaps:

- Video spawning is stubbed (logs a message, does nothing)
- No UI for configuring `SandboxLimits` at runtime
- Texture upload, AudioClip creation, and quad spawning are minimal/unpolished
- Error handling surfaces to the Console only — no in-editor UI feedback
- `strip_metadata` does not support TIFF (IFD structure is interwoven with image data; use decode→`encode_image` round-trip instead)

## Future Work

Items that are known, understood, and explicitly deferred:

**Decode performance**
- *Per-format target-sized decode* — large images are decoded at full resolution then scaled down. `zune-jpeg` uses 1/8, 1/4, 1/2 IDCT variants internally but does not expose a scale-factor in its 0.5 public API; PNG filter dependencies require every row regardless. Both remain full-resolution for now. See `decoder/src/img.rs`.

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

The bundled Wasmtime native libraries (`unity_package/Plugins/`) are distributed under the [Apache 2.0 license](https://github.com/bytecodealliance/wasmtime/blob/main/LICENSE).
