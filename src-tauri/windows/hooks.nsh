; Runs right after the installer copies the app files.
; The app itself downloads the download engine and verifies it against the
; hashes compiled into the app (see engine-manifest.json). If this step fails
; (offline, antivirus, ...), installation still succeeds and the app repairs
; the engine on first launch.
!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Setting up components..."
  nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --install-engine'
  Pop $0
  DetailPrint "Component setup finished (code $0)"
!macroend
