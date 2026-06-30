#if UNITY_EDITOR
using System;
using System.Collections.Generic;
using System.IO;
using Newtonsoft.Json.Linq;
using System.Threading.Tasks;
using UnityEditor;
using UnityEngine;
using xyz.yewnyx.MediaSandbox;

/// <summary>
/// Editor window for querying and displaying EXIF and XMP metadata from image files
/// via the decoder.wasm sandbox.
///
/// Open via  MediaSandbox ▶ Inspect Metadata…
/// Requires Play mode — the WASM engine lives on the MediaDecoderSandbox component
/// that InitializeMediaSandbox creates on play entry.
/// </summary>
public sealed class MediaMetadataInspector : EditorWindow
{
    // ── Styles (lazy, rebuilt after domain reload) ────────────────────────────

    static GUIStyle _tagKeyStyle;
    static GUIStyle _tagValStyle;
    static GUIStyle _xmpStyle;

    static GUIStyle TagKeyStyle => _tagKeyStyle ??= new GUIStyle(EditorStyles.miniLabel)
        { fontStyle = FontStyle.Bold, wordWrap = false };

    static GUIStyle TagValStyle => _tagValStyle ??= new GUIStyle(EditorStyles.miniLabel)
        { wordWrap = false };

    static GUIStyle XmpStyle => _xmpStyle ??= new GUIStyle(EditorStyles.textArea)
        { wordWrap = true, fontSize = 10, richText = false };

    // ── Per-window state ──────────────────────────────────────────────────────

    string _filePath = "";
    bool   _loading;
    string _errorMsg;

    Dictionary<string, string> _exif;
    string _xmpPacket;

    bool    _xmpFoldout;
    Vector2 _exifScroll;
    Vector2 _xmpScroll;

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    [MenuItem("MediaSandbox/Inspect Metadata…")]
    static void OpenWindow() => GetWindow<MediaMetadataInspector>("Metadata Inspector").Show();

    void OnEnable()
    {
        EditorApplication.playModeStateChanged += OnPlayModeChanged;
        minSize = new Vector2(360, 200);
    }

    void OnDisable()
    {
        EditorApplication.playModeStateChanged -= OnPlayModeChanged;
    }

    void OnPlayModeChanged(PlayModeStateChange _) => Repaint();

    // ── GUI ───────────────────────────────────────────────────────────────────

    void OnGUI()
    {
        DrawToolbar();

        if (!Application.isPlaying)
        {
            EditorGUILayout.HelpBox(
                "Enter Play mode to query metadata (the WASM engine is instantiated on play entry).",
                MessageType.Info);
            return;
        }

        if (_loading)
        {
            GUILayout.FlexibleSpace();
            EditorGUILayout.LabelField("Querying…", EditorStyles.centeredGreyMiniLabel);
            GUILayout.FlexibleSpace();
            return;
        }

        if (_errorMsg != null)
        {
            EditorGUILayout.HelpBox(_errorMsg, MessageType.Error);
        }

        if (_exif == null && _xmpPacket == null)
        {
            if (_errorMsg == null)
            {
                GUILayout.FlexibleSpace();
                EditorGUILayout.LabelField(
                    "Pick a file and press Query.", EditorStyles.centeredGreyMiniLabel);
                GUILayout.FlexibleSpace();
            }
            return;
        }

        DrawExifSection();
        DrawXmpSection();
    }

    void DrawToolbar()
    {
        using (new EditorGUILayout.HorizontalScope(EditorStyles.toolbar))
        {
            _filePath = EditorGUILayout.TextField(_filePath, EditorStyles.toolbarTextField);

            if (GUILayout.Button("…", EditorStyles.toolbarButton, GUILayout.Width(22)))
            {
                var path = EditorUtility.OpenFilePanel(
                    "Open image", "", "png,jpg,jpeg,tif,tiff,bmp,webp,qoi,hdr,gif");
                if (!string.IsNullOrEmpty(path))
                {
                    _filePath  = path;
                    _exif      = null;
                    _xmpPacket = null;
                    _errorMsg  = null;
                    Repaint();
                }
            }

            EditorGUI.BeginDisabledGroup(
                _loading || !Application.isPlaying || string.IsNullOrEmpty(_filePath));

            if (GUILayout.Button("Query", EditorStyles.toolbarButton, GUILayout.Width(46)))
                _ = RunQueryAsync();

            EditorGUI.EndDisabledGroup();
        }
    }

    void DrawExifSection()
    {
        EditorGUILayout.LabelField("EXIF", EditorStyles.boldLabel);

        if (_exif == null || _exif.Count == 0)
        {
            EditorGUILayout.LabelField("  (none)", EditorStyles.centeredGreyMiniLabel);
            return;
        }

        // Fixed-height scroll region — proportional to window but capped
        float exifHeight = Mathf.Min(_exif.Count * (EditorGUIUtility.singleLineHeight + 2) + 4, 220);
        _exifScroll = EditorGUILayout.BeginScrollView(
            _exifScroll, GUIStyle.none, GUI.skin.verticalScrollbar,
            GUILayout.Height(exifHeight));

        foreach (var (key, value) in _exif)
        {
            using (new EditorGUILayout.HorizontalScope())
            {
                EditorGUILayout.LabelField(key,   TagKeyStyle, GUILayout.Width(170));
                EditorGUILayout.LabelField(value, TagValStyle, GUILayout.ExpandWidth(true));
            }
        }

        EditorGUILayout.EndScrollView();
    }

    void DrawXmpSection()
    {
        EditorGUILayout.Space(6);

        string foldLabel = (_xmpPacket == null || _xmpPacket.Length == 0)
            ? "XMP  (none)"
            : $"XMP  ({_xmpPacket.Length:N0} chars)";

        _xmpFoldout = EditorGUILayout.Foldout(_xmpFoldout, foldLabel, toggleOnLabelClick: true);

        if (!_xmpFoldout || string.IsNullOrEmpty(_xmpPacket)) return;

        float remaining = position.height - GUILayoutUtility.GetLastRect().yMax - 30;
        float xmpHeight = Mathf.Max(80, remaining);

        _xmpScroll = EditorGUILayout.BeginScrollView(
            _xmpScroll, GUILayout.Height(xmpHeight));

        EditorGUILayout.TextArea(_xmpPacket, XmpStyle, GUILayout.ExpandHeight(true));

        EditorGUILayout.EndScrollView();

        if (GUILayout.Button("Copy XMP to Clipboard", GUILayout.Width(170)))
            GUIUtility.systemCopyBuffer = _xmpPacket;
    }

    // ── Query logic ───────────────────────────────────────────────────────────

    async Task RunQueryAsync()
    {
        _loading   = true;
        _errorMsg  = null;
        _exif      = null;
        _xmpPacket = null;
        Repaint();

        try
        {
            byte[] data;
            try { data = File.ReadAllBytes(_filePath); }
            catch (Exception ex) { _errorMsg = $"Read error: {ex.Message}"; return; }

            var sandbox = FindFirstObjectByType<MediaDecoderSandbox>();
            if (sandbox == null)
            {
                _errorMsg =
                    "MediaDecoderSandbox not found. The [SYSTEM] MediaSandbox " +
                    "GameObject should have been created by InitializeMediaSandbox on play entry.";
                return;
            }

            string json = await sandbox.QueryMetadataAsync(data);
            ParseMetadataJson(json, out _exif, out _xmpPacket);
        }
        catch (Exception ex)
        {
            _errorMsg = ex.Message;
        }
        finally
        {
            _loading = false;
            Repaint();
        }
    }

    // ── JSON parsing ──────────────────────────────────────────────────────────

    static void ParseMetadataJson(
        string json,
        out Dictionary<string, string> exif,
        out string xmpPacket)
    {
        exif      = new Dictionary<string, string>();
        xmpPacket = null;

        if (string.IsNullOrEmpty(json)) return;

        try
        {
            var root = JObject.Parse(json);

            if (root["exif"] is JObject exifObj)
                foreach (var prop in exifObj.Properties())
                    exif[prop.Name] = prop.Value.Value<string>() ?? "";

            if (root["xmp_packet"] is JValue xmpVal)
                xmpPacket = xmpVal.Value<string>() ?? "";
        }
        catch
        {
            // Malformed JSON — surface nothing rather than crash
        }
    }
}
#endif
