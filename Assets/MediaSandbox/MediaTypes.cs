using System;

namespace xyz.yewnyx.MediaSandbox
{
    public enum MediaType
    {
        Unknown   = 0,
        Image     = 1,
        Animation = 2,
        Audio     = 3,
        Video     = 4, // future: Unity VideoPlayer, no WASM involvement
    }

    public enum ImageFormat
    {
        Png  = 0,
        Jpeg = 1,
    }

    public readonly struct MediaAttributes
    {
        public readonly MediaType Type;
        public readonly int    Width;
        public readonly int    Height;
        public readonly int    FrameCount;      // 1 for stills
        public readonly int    SampleRate;
        public readonly int    ChannelCount;
        public readonly long   DurationMs;
        public readonly long   RequiredBufferSize;
        /// <summary>
        /// True when the format may carry an alpha channel (PNG, WebP, QOI, GIF, TIFF).
        /// False for formats that are structurally opaque (JPEG, HDR, BMP).
        /// A true value means alpha should be checked after decode; it does not
        /// guarantee that any pixel is actually transparent.
        /// </summary>
        public readonly bool   CanHaveAlpha;

        public MediaAttributes(
            MediaType type, int width, int height, int frameCount,
            int sampleRate, int channelCount, long durationMs,
            long requiredBufferSize, bool canHaveAlpha)
        {
            Type               = type;
            Width              = width;
            Height             = height;
            FrameCount         = frameCount;
            SampleRate         = sampleRate;
            ChannelCount       = channelCount;
            DurationMs         = durationMs;
            RequiredBufferSize = requiredBufferSize;
            CanHaveAlpha       = canHaveAlpha;
        }
    }

    public readonly struct RawImageData
    {
        public readonly int    Width;
        public readonly int    Height;
        public readonly byte[] Rgba;

        public RawImageData(int width, int height, byte[] rgba)
        {
            Width  = width;
            Height = height;
            Rgba   = rgba;
        }
    }

    public readonly struct AnimationFrame
    {
        public readonly byte[] Rgba;
        public readonly int    DelayMs;

        public AnimationFrame(byte[] rgba, int delayMs)
        {
            Rgba    = rgba;
            DelayMs = delayMs;
        }
    }

    public sealed class AnimatedImageData
    {
        public readonly int              Width;
        public readonly int              Height;
        public readonly AnimationFrame[] Frames;

        public AnimatedImageData(int width, int height, AnimationFrame[] frames)
        {
            Width  = width;
            Height = height;
            Frames = frames;
        }
    }

    public readonly struct RawAudioData
    {
        public readonly float[] Samples;
        public readonly int     SampleRate;
        public readonly int     Channels;

        public RawAudioData(float[] samples, int sampleRate, int channels)
        {
            Samples    = samples;
            SampleRate = sampleRate;
            Channels   = channels;
        }
    }

    public sealed class PathologicalMediaException : Exception
    {
        public PathologicalMediaException(string message) : base(message) { }
    }

    public static class SandboxLimits
    {
        // Images/animations larger than this in either dimension are scaled down to fit.
        // Full decode still happens first; see the TODO in decoder/src/img.rs for the
        // future per-format fast path that avoids the full-resolution intermediate.
        public static int  MaxDecodeDimension = 8_192;
        // Reject any file larger than this before WASM decode
        public static long MaxFileSizeBytes   = 512L * 1024 * 1024;
        // No duration limit: file size covers the pathological case
    }
}
