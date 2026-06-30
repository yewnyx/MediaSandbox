#if UNITY_EDITOR
using UnityEditor;
using UnityEngine;

namespace xyz.yewnyx.MediaSandboxExample
{
    [InitializeOnLoad]
    static class InitializeMediaSandbox
    {
        static InitializeMediaSandbox()
        {
            EditorApplication.playModeStateChanged += OnPlayModeChanged;
        }

        private static void OnPlayModeChanged(PlayModeStateChange state)
        {
            if (state == PlayModeStateChange.EnteredPlayMode)
            {
                var go = new GameObject("[SYSTEM] MediaSandbox");
                go.AddComponent<MediaDecoderSandbox>();
                go.AddComponent<DragDropMediaSpawner>();
                Object.DontDestroyOnLoad(go);
                MediaDropWindow.Open();
            }
            else if (state == PlayModeStateChange.ExitingPlayMode)
            {
                MediaDropWindow.TryClose();
            }
        }

        [MenuItem("MediaSandbox/Open Media File...")]
        private static void OpenMediaFile()
        {
            var spawner = Object.FindFirstObjectByType<DragDropMediaSpawner>();
            if (spawner == null)
            {
                EditorUtility.DisplayDialog("MediaSandbox", "Enter Play mode first.", "OK");
                return;
            }

            var path = EditorUtility.OpenFilePanel("Open Media File", "", "png,jpg,jpeg,gif,webp,mp3,flac,wav,ogg,mp4,webm,mkv");
            if (string.IsNullOrEmpty(path)) return;

            spawner.HandleDrop(path);
        }
    }
}
#endif
