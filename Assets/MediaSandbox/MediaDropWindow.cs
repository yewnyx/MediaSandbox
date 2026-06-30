#if UNITY_EDITOR
using UnityEditor;
using UnityEngine;

namespace xyz.yewnyx.MediaSandboxExample
{
    public sealed class MediaDropWindow : EditorWindow
    {
        private static MediaDropWindow _instance;

        public static void Open()
        {
            if (_instance != null) { _instance.Focus(); return; }
            _instance = CreateInstance<MediaDropWindow>();
            _instance.titleContent = new GUIContent("Media Drop");
            _instance.minSize = new Vector2(180, 44);
            _instance.maxSize = new Vector2(600, 44);
            _instance.ShowUtility();
            // Position in the top-right of the main Unity window
            var main = EditorGUIUtility.GetMainWindowPosition();
            _instance.position = new Rect(main.xMax - 340, main.y + 30, 320, 44);
        }

        public static void TryClose()
        {
            if (_instance != null)
                _instance.Close();
        }

        private void OnGUI()
        {
            EditorGUI.DrawRect(new Rect(0, 0, position.width, position.height), new Color(0.12f, 0.12f, 0.12f));
            var labelStyle = new GUIStyle(EditorStyles.centeredGreyMiniLabel) { fontSize = 11 };
            GUILayout.FlexibleSpace();
            GUILayout.Label("Drop media files here (PNG, JPEG, GIF, MP3, FLAC, WAV, …)", labelStyle);
            GUILayout.FlexibleSpace();

            var e = Event.current;
            if (e.type != EventType.DragUpdated && e.type != EventType.DragPerform) return;

            DragAndDrop.visualMode = DragAndDropVisualMode.Copy;

            if (e.type == EventType.DragPerform)
            {
                DragAndDrop.AcceptDrag();
                var spawner = Object.FindFirstObjectByType<DragDropMediaSpawner>();
                if (spawner == null)
                {
                    Debug.LogWarning("[MediaSandbox] No DragDropMediaSpawner active — enter Play mode first");
                    e.Use();
                    return;
                }
                foreach (var path in DragAndDrop.paths)
                    spawner.HandleDrop(path);
            }
            e.Use();
        }

        private void OnDestroy() => _instance = null;
    }
}
#endif
