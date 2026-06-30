namespace xyz.yewnyx.MediaSandboxExample
{
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
