using System;
using System.Collections;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using UnityEngine;

namespace xyz.yewnyx.MediaSandbox
{
    /// <summary>
    /// Handles drag-and-drop of media files onto the Game window in Editor play mode.
    /// Spawns a textured Quad for images/animations, or an AudioSource for audio.
    /// </summary>
    [RequireComponent(typeof(MediaDecoderSandbox))]
    public sealed class DragDropMediaSpawner : MonoBehaviour
    {
        // Set to true to log a line after every decoded frame instead of every 10%.
        private const bool VerboseDecodeLogging = false;
        private MediaDecoderSandbox _sandbox;
        private CancellationTokenSource _cts;

        private void Awake()
        {
            _sandbox = GetComponent<MediaDecoderSandbox>();
            _cts     = new CancellationTokenSource();
        }

        private void OnDestroy() => _cts?.Cancel();

        public async void HandleDrop(string path)
        {
            byte[] data;
            try { data = File.ReadAllBytes(path); }
            catch (Exception ex)
            {
                Debug.LogError($"[MediaSandbox] Failed to read '{path}': {ex.Message}");
                return;
            }

            var ct = _cts.Token;

            MediaAttributes attrs;
            try
            {
                attrs = await _sandbox.QueryAttributesAsync(data, ct);
            }
            catch (OperationCanceledException) { return; }
            catch (Exception ex)
            {
                Debug.LogError($"[MediaSandbox] QueryAttributes failed for '{path}': {ex.Message}");
                return;
            }

            var fileName = System.IO.Path.GetFileName(path);

            switch (attrs.Type)
            {
                case MediaType.Image:
                    await SpawnImageAsync(data, attrs, fileName, ct);
                    break;

                case MediaType.Animation:
                    await SpawnAnimationAsync(data, attrs, fileName, ct);
                    break;

                case MediaType.Audio:
                    await SpawnAudioAsync(data, fileName, ct);
                    break;

                case MediaType.Video:
                    Debug.Log($"[MediaSandbox] Video detected: '{fileName}' — VideoPlayer spawning not yet implemented");
                    break;

                default:
                    Debug.LogWarning($"[MediaSandbox] Unknown or unsupported format: '{fileName}'");
                    break;
            }
        }

        // ── Image ─────────────────────────────────────────────────────────────

        private async System.Threading.Tasks.Task SpawnImageAsync(
            byte[] data, MediaAttributes attrs, string name, CancellationToken ct)
        {
            RawImageData raw;
            try { raw = await _sandbox.DecodeImageAsync(data, ct); }
            catch (PathologicalMediaException ex)
            {
                Debug.LogError($"[MediaSandbox] Rejected '{name}': {ex.Message}");
                return;
            }
            catch (OperationCanceledException) { return; }
            catch (Exception ex)
            {
                Debug.LogError($"[MediaSandbox] DecodeImage failed for '{name}': {ex.Message}");
                return;
            }

            // Back on main thread after await — Unity API safe
            using (raw)
                SpawnTexturedQuad(name, raw.Width, raw.Height, raw.Rgba, attrs.CanHaveAlpha);
        }

        // ── Animation ─────────────────────────────────────────────────────────

        private async System.Threading.Tasks.Task SpawnAnimationAsync(
            byte[] data, MediaAttributes attrs, string name, CancellationToken ct)
        {
            Debug.Log($"[MediaSandbox] Decoding '{name}' — {attrs.FrameCount} frames ({attrs.Width}×{attrs.Height})...");

            int lastReportedPct = -1;
            var progress = new Progress<float>(f => {
                if (VerboseDecodeLogging)
                {
                    int frame = Mathf.RoundToInt(f * attrs.FrameCount);
                    Debug.Log($"[MediaSandbox] '{name}': frame {frame}/{attrs.FrameCount}");
                }
                else
                {
                    int pct = (int)(f * 100f / 10) * 10; // floor to nearest 10%
                    if (pct <= lastReportedPct) return;
                    lastReportedPct = pct;
                    Debug.Log($"[MediaSandbox] Decoding '{name}': {pct}%");
                }
            });

            AnimatedImageData anim;
            try { anim = await _sandbox.DecodeAnimationAsync(data, progress, ct); }
            catch (PathologicalMediaException ex)
            {
                Debug.LogError($"[MediaSandbox] Rejected '{name}': {ex.Message}");
                return;
            }
            catch (OperationCanceledException) { return; }
            catch (Exception ex)
            {
                Debug.LogError($"[MediaSandbox] DecodeAnimation failed for '{name}': {ex.Message}");
                return;
            }

            Debug.Log($"[MediaSandbox] Decoded '{name}' — {anim.Frames.Length} frames");

            using (anim)
            {
                if (anim.Frames.Length == 1)
                    SpawnTexturedQuad(name, anim.Width, anim.Height, anim.Frames[0].Rgba, attrs.CanHaveAlpha);
                else
                    SpawnAnimatedQuad(name, anim, attrs.CanHaveAlpha);
            }
        }

        // ── Audio ─────────────────────────────────────────────────────────────

        private async System.Threading.Tasks.Task SpawnAudioAsync(
            byte[] data, string name, CancellationToken ct)
        {
            Debug.Log($"[MediaSandbox] Decoding audio '{name}'...");
            RawAudioData raw;
            try { raw = await _sandbox.DecodeAudioAsync(data, ct); }
            catch (PathologicalMediaException ex)
            {
                Debug.LogError($"[MediaSandbox] Rejected '{name}': {ex.Message}");
                return;
            }
            catch (OperationCanceledException) { return; }
            catch (Exception ex)
            {
                Debug.LogError($"[MediaSandbox] DecodeAudio failed for '{name}': {ex.Message}");
                return;
            }

            // Main thread — create AudioClip and play
            var clip = AudioClip.Create(name, raw.Samples.Length / raw.Channels,
                raw.Channels, raw.SampleRate, stream: false);
            clip.SetData(raw.Samples, offsetSamples: 0);

            var go = new GameObject($"[Audio] {name}");
            var src = go.AddComponent<AudioSource>();
            src.clip = clip;
            src.Play();
            Debug.Log($"[MediaSandbox] Playing '{name}' — {raw.Channels}ch {raw.SampleRate}Hz " +
                      $"({raw.Samples.Length / raw.Channels / (float)raw.SampleRate:F1}s)");
        }

        // ── Helpers ───────────────────────────────────────────────────────────

        private static Vector3 AspectScale(int width, int height) =>
            new Vector3(width / (float)height, 1f, 1f);

        /// Scans RGBA bytes for any pixel with alpha < 255. Early-exits on first hit.
        private static bool HasAnyTransparency(ReadOnlyMemory<byte> rgba) => HasAnyTransparency(rgba.Span);
        private static bool HasAnyTransparency(ReadOnlySpan<byte> rgba)
        {
            for (int i = 3; i < rgba.Length; i += 4)
                if (rgba[i] < 255) return true;
            return false;
        }

        private static void SpawnTexturedQuad(string name, int width, int height, ReadOnlyMemory<byte> rgba, bool canHaveAlpha)
        {
            // linear: false — sRGB-encoded input; GPU linearises during sampling in a linear-space project.
            var tex = new Texture2D(width, height, TextureFormat.RGBA32, mipChain: false, linear: false);
            // LoadRawTextureData(byte[]) uses array.Length, which may exceed our slice if the buffer
            // is pooled. Pin the underlying array and use the (IntPtr, int) overload instead.
            MemoryMarshal.TryGetArray(rgba, out var seg);
            var pin = GCHandle.Alloc(seg.Array, GCHandleType.Pinned);
            try   { tex.LoadRawTextureData(pin.AddrOfPinnedObject() + seg.Offset, seg.Count); }
            finally { pin.Free(); }
            tex.Apply();

            var go  = GameObject.CreatePrimitive(PrimitiveType.Quad);
            go.name = $"[Image] {name}";
            go.transform.position   = Vector3.zero;
            go.transform.localScale = AspectScale(width, height);

            bool hasAlpha  = canHaveAlpha && HasAnyTransparency(rgba.Span);
            var shaderName = hasAlpha ? "Unlit/Transparent" : "Unlit/Texture";
            var mat        = new Material(Shader.Find(shaderName));
            mat.mainTexture = tex;
            go.GetComponent<MeshRenderer>().material = mat;

            Debug.Log($"[MediaSandbox] Spawned image '{name}' ({width}×{height}{(hasAlpha ? ", alpha" : "")})");
        }

        private void SpawnAnimatedQuad(string name, AnimatedImageData anim, bool canHaveAlpha)
        {
            var go  = GameObject.CreatePrimitive(PrimitiveType.Quad);
            go.name = $"[Anim] {name}";
            go.transform.position   = Vector3.zero;
            go.transform.localScale = AspectScale(anim.Width, anim.Height);

            // Scan the first frame to decide the shader; assume all frames share the same alpha state.
            bool hasAlpha  = canHaveAlpha && anim.Frames.Length > 0 && HasAnyTransparency(anim.Frames[0].Rgba);
            var shaderName = hasAlpha ? "Unlit/Transparent" : "Unlit/Texture";
            var mat        = new Material(Shader.Find(shaderName));
            go.GetComponent<MeshRenderer>().material = mat;

            // Bake all frame textures up front; extract delays before releasing pooled buffers.
            // linear: false — sRGB-encoded input; GPU linearises during sampling in a linear-space project.
            var textures = new Texture2D[anim.Frames.Length];
            var delaysMs = new int[anim.Frames.Length];
            for (int i = 0; i < anim.Frames.Length; i++)
            {
                var t = new Texture2D(anim.Width, anim.Height, TextureFormat.RGBA32, mipChain: false, linear: false);
                MemoryMarshal.TryGetArray(anim.Frames[i].Rgba, out var seg);
                var pin = GCHandle.Alloc(seg.Array, GCHandleType.Pinned);
                try   { t.LoadRawTextureData(pin.AddrOfPinnedObject() + seg.Offset, seg.Count); }
                finally { pin.Free(); }
                t.Apply();
                textures[i] = t;
                delaysMs[i] = anim.Frames[i].DelayMs;
            }
            // All RGBA data is now on the GPU — return pooled buffers before starting the coroutine.
            anim.Dispose();

            Debug.Log($"[MediaSandbox] Spawned animation '{name}' ({anim.Width}×{anim.Height}, " +
                      $"{anim.Frames.Length} frames{(hasAlpha ? ", alpha" : "")})");
            StartCoroutine(AnimateQuad(mat, textures, delaysMs, _cts.Token));
        }

        private static IEnumerator AnimateQuad(
            Material mat, Texture2D[] textures, int[] delaysMs, CancellationToken ct)
        {
            int i = 0;
            while (!ct.IsCancellationRequested)
            {
                mat.mainTexture = textures[i];
                float delay = delaysMs[i] / 1000f;
                if (delay <= 0f) delay = 0.1f;
                yield return new WaitForSeconds(delay);
                i = (i + 1) % textures.Length;
            }
        }
    }
}
