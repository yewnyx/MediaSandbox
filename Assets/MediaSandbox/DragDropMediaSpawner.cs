using System;
using System.Collections;
using System.IO;
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
            SpawnTexturedQuad(name, raw.Width, raw.Height, raw.Rgba);
        }

        // ── Animation ─────────────────────────────────────────────────────────

        private async System.Threading.Tasks.Task SpawnAnimationAsync(
            byte[] data, MediaAttributes attrs, string name, CancellationToken ct)
        {
            AnimatedImageData anim;
            try { anim = await _sandbox.DecodeAnimationAsync(data, ct); }
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

            if (anim.Frames.Length == 1)
            {
                // Single-frame animation — treat as still image
                SpawnTexturedQuad(name, anim.Width, anim.Height, anim.Frames[0].Rgba);
            }
            else
            {
                SpawnAnimatedQuad(name, anim);
            }
        }

        // ── Audio ─────────────────────────────────────────────────────────────

        private async System.Threading.Tasks.Task SpawnAudioAsync(
            byte[] data, string name, CancellationToken ct)
        {
            RawAudioData raw;
            try { raw = await _sandbox.DecodeAudioAsync(data, ct: ct); }
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

        private static void SpawnTexturedQuad(string name, int width, int height, byte[] rgba)
        {
            var tex = new Texture2D(width, height, TextureFormat.RGBA32, mipChain: false);
            tex.LoadRawTextureData(rgba);
            tex.Apply();

            var go  = GameObject.CreatePrimitive(PrimitiveType.Quad);
            go.name = $"[Image] {name}";
            go.transform.position = Vector3.zero;

            var mat = new Material(Shader.Find("Unlit/Texture"));
            mat.mainTexture = tex;
            go.GetComponent<MeshRenderer>().material = mat;

            Debug.Log($"[MediaSandbox] Spawned image '{name}' ({width}×{height})");
        }

        private void SpawnAnimatedQuad(string name, AnimatedImageData anim)
        {
            var go  = GameObject.CreatePrimitive(PrimitiveType.Quad);
            go.name = $"[Anim] {name}";
            go.transform.position = Vector3.zero;

            var mat = new Material(Shader.Find("Unlit/Texture"));
            go.GetComponent<MeshRenderer>().material = mat;

            // Bake all frame textures up front
            var textures = new Texture2D[anim.Frames.Length];
            for (int i = 0; i < anim.Frames.Length; i++)
            {
                var t = new Texture2D(anim.Width, anim.Height, TextureFormat.RGBA32, mipChain: false);
                t.LoadRawTextureData(anim.Frames[i].Rgba);
                t.Apply();
                textures[i] = t;
            }

            Debug.Log($"[MediaSandbox] Spawned animation '{name}' ({anim.Width}×{anim.Height}, {anim.Frames.Length} frames)");
            StartCoroutine(AnimateQuad(mat, textures, anim.Frames, _cts.Token));
        }

        private static IEnumerator AnimateQuad(
            Material mat, Texture2D[] textures, AnimationFrame[] frames, CancellationToken ct)
        {
            int i = 0;
            while (!ct.IsCancellationRequested)
            {
                mat.mainTexture = textures[i];
                float delay = frames[i].DelayMs / 1000f;
                if (delay <= 0f) delay = 0.1f;
                yield return new WaitForSeconds(delay);
                i = (i + 1) % textures.Length;
            }
        }
    }
}
