#if UNITY_EDITOR
using UnityEditor;
using UnityEngine;

namespace xyz.yewnyx.MediaSandbox
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
    }
}
#endif
