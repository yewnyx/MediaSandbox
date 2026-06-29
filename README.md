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
- Rejects pathological inputs before decode: files over 512 MB or images over 8192 px in either dimension throw `PathologicalMediaException`

## Getting Started

Requirements:

- Unity 2022.3 or later (tested on 2022.3 LTS)
- The `Wasmtime` NuGet package is vendored under `Assets/Packages/` — no additional install step
- `wasmtime.dll` (native runtime) is under `Assets/Plugins/`
- The compiled `decoder.wasm` lives at `Assets/media~/decoder.wasm`

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
cd decoder
.\build.ps1
```

This compiles the Rust crate to `wasm32-wasip1` with `opt-level = "z"`, LTO, and symbol stripping, then copies the output to `Assets/media~/decoder.wasm`. You need the Rust toolchain with the `wasm32-wasip1` target installed:

```
rustup target add wasm32-wasip1
```

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

## License

MIT
