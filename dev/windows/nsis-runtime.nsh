; Bundled GStreamer and MSVC runtime for the NSIS installer.
;
; The Dioxus NSIS template !includes this file at top level and does not invoke
; hook macros, so the installer work lives in a hidden section. The section is
; always installed and the existing uninstaller removes it with
; `RMDir /r "$INSTDIR"`.
;
; `dev/prepare-windows-runtime.ps1` stages the files before `dx bundle`.
; The path is relative to the repository root, where `dx bundle` runs in CI
; and in the documented local build. The previous `${__FILEDIR__}/../..`
; absolute path was not resolved by makensis on Windows.

!define JABU_RUNTIME "dist/windows-runtime"

Section "-GStreamer and MSVC runtime"
    ; Core DLLs, the plugin scanner's dependencies and the MSVC CRT/OpenMP
    ; runtime all have to sit next to jabu.exe: Windows resolves the import
    ; table before main() runs.
    SetOutPath "$INSTDIR"
    File "${JABU_RUNTIME}/bin/*.dll"

    ; Dioxus 0.7.10 copies external_bin without the `.exe` suffix on Windows.
    ; Rename the helper so SidecarConfig::default_helper_path() finds it. A
    ; future bundler that keeps the suffix leaves the existing `.exe` alone.
    IfFileExists "$INSTDIR\jabu-translate-helper.exe" jabu_helper_ready
    IfFileExists "$INSTDIR\jabu-translate-helper" 0 +2
    Rename "$INSTDIR\jabu-translate-helper" "$INSTDIR\jabu-translate-helper.exe"
    jabu_helper_ready:

    ; GStreamer discovers the plugins from the prefix derived from the
    ; libgstreamer DLL, which is the application directory in this layout.
    SetOutPath "$INSTDIR/lib/gstreamer-1.0"
    File "${JABU_RUNTIME}/lib/gstreamer-1.0/*.dll"

    ; The scanner and its helper are launched as child processes.
    SetOutPath "$INSTDIR/libexec/gstreamer-1.0"
    File /r "${JABU_RUNTIME}/libexec/gstreamer-1.0/*"

    SetOutPath "$INSTDIR"
SectionEnd
