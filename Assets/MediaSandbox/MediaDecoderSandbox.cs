using System;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;
using Wasmtime;

namespace xyz.yewnyx.MediaSandbox
{
    /// <summary>
    /// Wraps the decoder.wasm sandbox. Engine, Linker, and Module are shared across calls;
    /// each async operation creates its own Store + Instance so decodes run fully concurrently.
    /// Unity types (Texture2D, AudioClip) are NOT created here — callers must do that on the main thread.
    /// </summary>
    public sealed class MediaDecoderSandbox : MonoBehaviour
    {
        // Shared (thread-safe)
        private Engine _engine;
        private Linker _linker;
        private Module _module;

        // ── AttrResult layout must match Rust #[repr(C)] exactly ─────────────
        // Offsets: duration_ms(0) required_buffer_size(8) media_type(16) width(20)
        //          height(24) frame_count(28) sample_rate(32) channel_count(36)
        //          page_count(40) error_code(44)   Total: 48 bytes
        private const int AttrResultSize = 48;

        private void Awake()
        {
#if !MEDIA_SANDBOX_NO_PULLEY
            // Pulley is Wasmtime's portable bytecode interpreter — required on platforms
            // where JIT is prohibited (iOS). Activated by setting target to "pulley64".
            // Define MEDIA_SANDBOX_NO_PULLEY to fall back to Cranelift JIT instead.
            //
            // The native wasmtime library must be built with the `pulley` cargo feature.
            // See .github/workflows/build.yml and scripts/build-wasmtime-*.sh.
            _engine = new Engine(MakePulleyConfig());
            if (!_engine.IsPulleyInterpreter)
                throw new InvalidOperationException(
                    "[MediaSandbox] Expected Pulley interpreter but Wasmtime is using Cranelift. " +
                    "Ensure the native wasmtime library was built with --features pulley, " +
                    "or define MEDIA_SANDBOX_NO_PULLEY to use Cranelift explicitly.");
#else
            _engine = new Engine();
#endif
            _linker = new Linker(_engine);
            _linker.DefineWasi();
            _module = Module.FromFile(_engine, WasmPath);
        }

        // ── Pulley config shim ────────────────────────────────────────────────
        // Config.WithTarget() is absent from Wasmtime .NET v44's public API.
        // We call wasmtime_config_target_set from the C API directly, reaching
        // the raw wasm_config_t* via reflection on the SDK's private handle field.

        [DllImport("wasmtime", EntryPoint = "wasmtime_config_target_set",
                   CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr NativeConfigTargetSet(
            IntPtr config, [MarshalAs(UnmanagedType.LPUTF8Str)] string? target);

        private static Config MakePulleyConfig()
        {
            var config = new Config();

            // Walk the SDK's private fields to find the raw wasm_config_t* handle.
            IntPtr nativeHandle = IntPtr.Zero;
            const System.Reflection.BindingFlags flags =
                System.Reflection.BindingFlags.NonPublic |
                System.Reflection.BindingFlags.Instance;

            foreach (var field in typeof(Config).GetFields(flags))
            {
                var val = field.GetValue(config);
                if (val is IntPtr p && p != IntPtr.Zero)
                {
                    nativeHandle = p;
                    break;
                }
                if (val is SafeHandle sh && !sh.IsInvalid)
                {
                    nativeHandle = sh.DangerousGetHandle();
                    break;
                }
                if (val != null && val.GetType().IsValueType)
                {
                    foreach (var inner in val.GetType().GetFields(flags))
                    {
                        if (inner.GetValue(val) is IntPtr ip && ip != IntPtr.Zero)
                        {
                            nativeHandle = ip;
                            break;
                        }
                    }
                    if (nativeHandle != IntPtr.Zero) break;
                }
            }

            if (nativeHandle == IntPtr.Zero)
                throw new InvalidOperationException(
                    "[MediaSandbox] Could not locate Wasmtime Config handle via reflection. " +
                    "The SDK's internal layout may have changed.");

            var err = NativeConfigTargetSet(nativeHandle, "pulley64");
            if (err != IntPtr.Zero)
                throw new InvalidOperationException(
                    "[MediaSandbox] wasmtime_config_target_set(\"pulley64\") failed.");

            return config;
        }

        private void OnDestroy()
        {
            _module?.Dispose();
            _linker?.Dispose();
            _engine?.Dispose();
        }

        private static string WasmPath =>
            System.IO.Path.Combine(Application.streamingAssetsPath, "mediasandbox", "decoder.wasm");

        // ── Per-call instance creation ────────────────────────────────────────

        private (Store store, Instance instance, Memory memory) CreateInstance()
        {
            var store    = new Store(_engine);
            store.SetWasiConfiguration(new WasiConfiguration());
            var instance = _linker.Instantiate(store, _module);
            var memory   = instance.GetMemory("memory")
                           ?? throw new InvalidOperationException("decoder.wasm has no exported 'memory'");
            return (store, instance, memory);
        }

        // ── Low-level helpers ─────────────────────────────────────────────────

        private static int WasmAlloc(Instance instance, int size)
        {
            return instance.GetFunction<int, int>("alloc")!(size);
        }

        private static void WasmDealloc(Instance instance, int ptr, int size)
        {
            instance.GetAction<int, int>("dealloc")!(ptr, size);
        }

        /// <summary>Copies data into WASM memory and returns the allocated pointer.</summary>
        private static int WasmWrite(Instance instance, Memory memory, ReadOnlySpan<byte> data)
        {
            int ptr = WasmAlloc(instance, data.Length);
            data.CopyTo(memory.GetSpan(ptr, data.Length));
            return ptr;
        }

        // ── Public async API ──────────────────────────────────────────────────

        /// <summary>
        /// Reads headers only; cheap. Also the routing point — result.Type tells the caller
        /// which decode method to use next.
        /// </summary>
        public Task<MediaAttributes> QueryAttributesAsync(
            ReadOnlyMemory<byte> data, CancellationToken ct = default)
        {
            return Task.Run(() => {
                var (store, instance, memory) = CreateInstance();
                using (store)
                {
                    return QueryAttributesCore(instance, memory, data.Span);
                }
            }, ct);
        }

        private static MediaAttributes QueryAttributesCore(
            Instance instance, Memory memory, ReadOnlySpan<byte> data)
        {
            int dataPtr = WasmWrite(instance, memory, data);
            int attrPtr = WasmAlloc(instance, AttrResultSize);
            try
            {
                instance.GetFunction<int, int, int, int>("query_attributes")!(dataPtr, data.Length, attrPtr);
                return ReadAttrResult(memory, attrPtr);
            }
            finally
            {
                WasmDealloc(instance, dataPtr, data.Length);
                WasmDealloc(instance, attrPtr, AttrResultSize);
            }
        }

        private static MediaAttributes ReadAttrResult(Memory memory, int ptr)
        {
            long durationMs         = memory.ReadInt64(ptr + 0);
            long requiredBufferSize = memory.ReadInt64(ptr + 8);
            int  mediaType          = memory.ReadInt32(ptr + 16);
            int  width              = memory.ReadInt32(ptr + 20);
            int  height             = memory.ReadInt32(ptr + 24);
            int  frameCount         = memory.ReadInt32(ptr + 28);
            int  sampleRate         = memory.ReadInt32(ptr + 32);
            int  channelCount       = memory.ReadInt32(ptr + 36);
            // page_count at 40 — reserved
            int  errorCode          = memory.ReadInt32(ptr + 44);

            if (errorCode != 0)
                throw new Exception($"query_attributes error code: {errorCode}");

            return new MediaAttributes(
                (MediaType)mediaType, width, height, frameCount,
                sampleRate, channelCount, durationMs, requiredBufferSize);
        }

        /// <summary>
        /// Decodes a static image (PNG, JPEG, BMP, TIFF, WebP, HDR, QOI) to raw RGBA bytes.
        /// Caller creates Texture2D on the main thread from the returned RawImageData.
        /// </summary>
        public Task<RawImageData> DecodeImageAsync(
            ReadOnlyMemory<byte> data, CancellationToken ct = default)
        {
            return Task.Run(async () => {
                var attrs = await QueryAttributesAsync(data, ct);

                if (attrs.Type != MediaType.Image)
                    throw new InvalidOperationException($"Expected Image, got {attrs.Type}");

                CheckPathological(attrs, data.Length);

                var (store, instance, memory) = CreateInstance();
                using (store)
                {
                    return DecodeImageCore(instance, memory, data.Span, attrs);
                }
            }, ct);
        }

        private static RawImageData DecodeImageCore(
            Instance instance, Memory memory, ReadOnlySpan<byte> data, MediaAttributes attrs)
        {
            int outLen  = (int)attrs.RequiredBufferSize;
            int dataPtr = WasmWrite(instance, memory, data);
            int outPtr  = WasmAlloc(instance, outLen);
            try
            {
                int code = instance.GetFunction<int, int, int, int, int>("decode_image")!
                    (dataPtr, data.Length, outPtr, outLen);
                if (code != 0) throw new Exception($"decode_image failed ({code})");

                var rgba = memory.GetSpan(outPtr, outLen).ToArray();
                return new RawImageData(attrs.Width, attrs.Height, rgba);
            }
            finally
            {
                WasmDealloc(instance, dataPtr, data.Length);
                WasmDealloc(instance, outPtr, outLen);
            }
        }

        /// <summary>
        /// Decodes an animation (GIF or WebP, static or animated).
        /// A single-frame result means the source was a still image — spawner handles both.
        /// </summary>
        public Task<AnimatedImageData> DecodeAnimationAsync(
            ReadOnlyMemory<byte> data, CancellationToken ct = default)
        {
            return Task.Run(async () => {
                var attrs = await QueryAttributesAsync(data, ct);

                if (attrs.Type != MediaType.Animation)
                    throw new InvalidOperationException($"Expected Animation, got {attrs.Type}");

                CheckPathological(attrs, data.Length);

                var (store, instance, memory) = CreateInstance();
                using (store)
                {
                    return DecodeAnimationCore(instance, memory, data.Span, attrs);
                }
            }, ct);
        }

        private static AnimatedImageData DecodeAnimationCore(
            Instance instance, Memory memory, ReadOnlySpan<byte> data, MediaAttributes attrs)
        {
            int outLen  = (int)attrs.RequiredBufferSize;
            int dataPtr = WasmWrite(instance, memory, data);
            int outPtr  = WasmAlloc(instance, outLen);
            try
            {
                int code = instance.GetFunction<int, int, int, int, int>("decode_animation")!
                    (dataPtr, data.Length, outPtr, outLen);
                if (code != 0) throw new Exception($"decode_animation failed ({code})");

                return ParseAnimationOutput(memory.GetSpan(outPtr, outLen), attrs.Width, attrs.Height);
            }
            finally
            {
                WasmDealloc(instance, dataPtr, data.Length);
                WasmDealloc(instance, outPtr, outLen);
            }
        }

        private static AnimatedImageData ParseAnimationOutput(
            Span<byte> output, int width, int height)
        {
            int frameCount = MemoryMarshal.Read<int>(output);
            int frameBytes = width * height * 4;
            var frames     = new AnimationFrame[frameCount];

            for (int i = 0; i < frameCount; i++)
            {
                int delayMs    = MemoryMarshal.Read<int>(output.Slice(4 + i * 4));
                int dataOffset = 4 + frameCount * 4 + i * frameBytes;
                var rgba       = output.Slice(dataOffset, frameBytes).ToArray();
                frames[i]      = new AnimationFrame(rgba, delayMs);
            }

            return new AnimatedImageData(width, height, frames);
        }

        /// <summary>
        /// Decodes audio to interleaved f32 PCM samples.
        /// Caller creates AudioClip on the main thread from the returned RawAudioData.
        /// </summary>
        public Task<RawAudioData> DecodeAudioAsync(
            ReadOnlyMemory<byte> data,
            IProgress<float> progress = null,
            CancellationToken ct = default)
        {
            return Task.Run(async () => {
                if (data.Length > SandboxLimits.MaxFileSizeBytes)
                    throw new PathologicalMediaException(
                        $"Audio file too large: {data.Length:N0} bytes (limit {SandboxLimits.MaxFileSizeBytes:N0})");

                var attrs = await QueryAttributesAsync(data, ct);
                if (attrs.Type != MediaType.Audio)
                    throw new InvalidOperationException($"Expected Audio, got {attrs.Type}");

                var (store, instance, memory) = CreateInstance();
                using (store)
                {
                    return DecodeAudioCore(instance, memory, data.Span);
                }
            }, ct);
        }

        private static RawAudioData DecodeAudioCore(
            Instance instance, Memory memory, ReadOnlySpan<byte> data)
        {
            int outPtrAddr = WasmAlloc(instance, 4);
            int outLenAddr = WasmAlloc(instance, 4);
            int srAddr     = WasmAlloc(instance, 4);
            int chAddr     = WasmAlloc(instance, 4);
            int dataPtr    = WasmWrite(instance, memory, data);
            try
            {
                int code = instance.GetFunction<int, int, int, int, int, int, int>("decode_audio")!
                    (dataPtr, data.Length, outPtrAddr, outLenAddr, srAddr, chAddr);
                if (code != 0) throw new Exception($"decode_audio failed ({code})");

                int sampleDataPtr = memory.ReadInt32(outPtrAddr);
                int sampleDataLen = memory.ReadInt32(outLenAddr);
                int sampleRate    = memory.ReadInt32(srAddr);
                int channels      = memory.ReadInt32(chAddr);

                int sampleCount = sampleDataLen / 4;
                var samples     = new float[sampleCount];
                MemoryMarshal
                    .Cast<byte, float>(memory.GetSpan(sampleDataPtr, sampleDataLen))
                    .CopyTo(samples);

                WasmDealloc(instance, sampleDataPtr, sampleDataLen);

                return new RawAudioData(samples, sampleRate, channels);
            }
            finally
            {
                WasmDealloc(instance, dataPtr, data.Length);
                WasmDealloc(instance, outPtrAddr, 4);
                WasmDealloc(instance, outLenAddr, 4);
                WasmDealloc(instance, srAddr, 4);
                WasmDealloc(instance, chAddr, 4);
            }
        }

        /// <summary>
        /// Encodes raw RGBA to the requested image format.
        /// Returns compressed bytes; does not interact with Unity types.
        /// </summary>
        public Task<byte[]> EncodeImageAsync(
            RawImageData image, ImageFormat format, CancellationToken ct = default)
        {
            return Task.Run(() => {
                var (store, instance, memory) = CreateInstance();
                using (store)
                {
                    return EncodeImageCore(instance, memory, image, format);
                }
            }, ct);
        }

        private static byte[] EncodeImageCore(
            Instance instance, Memory memory, RawImageData image, ImageFormat format)
        {
            int outPtrAddr = WasmAlloc(instance, 4);
            int outLenAddr = WasmAlloc(instance, 4);
            int rgbaPtr    = WasmWrite(instance, memory, image.Rgba);
            try
            {
                int code = instance.GetFunction<int, int, int, int, int, int, int>("encode_image")!
                    (rgbaPtr, image.Width, image.Height, (int)format, outPtrAddr, outLenAddr);
                if (code != 0) throw new Exception($"encode_image failed ({code})");

                int outPtr = memory.ReadInt32(outPtrAddr);
                int outLen = memory.ReadInt32(outLenAddr);
                var result = memory.GetSpan(outPtr, outLen).ToArray();

                WasmDealloc(instance, outPtr, outLen);
                return result;
            }
            finally
            {
                WasmDealloc(instance, rgbaPtr, image.Rgba.Length);
                WasmDealloc(instance, outPtrAddr, 4);
                WasmDealloc(instance, outLenAddr, 4);
            }
        }

        // ── Pathological guard ────────────────────────────────────────────────

        private static void CheckPathological(MediaAttributes attrs, long fileSize)
        {
            if (fileSize > SandboxLimits.MaxFileSizeBytes)
                throw new PathologicalMediaException(
                    $"File too large: {fileSize:N0} bytes (limit {SandboxLimits.MaxFileSizeBytes:N0})");

            if (attrs.Type == MediaType.Image || attrs.Type == MediaType.Animation)
            {
                if (attrs.Width > SandboxLimits.MaxImageDimension ||
                    attrs.Height > SandboxLimits.MaxImageDimension)
                    throw new PathologicalMediaException(
                        $"Image dimensions {attrs.Width}×{attrs.Height} exceed limit ({SandboxLimits.MaxImageDimension})");
            }
        }
    }
}
