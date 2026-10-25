; NSIS installer hooks.
;
; Install: nothing to do. aria2 is bundled as a sidecar next to the app
; executable. If it is ever missing or modified the app falls back to its own
; verified download into the per-user data directory and says so in the engine
; panel.
;
; Uninstall: the installer directory (including the bundled engine) is removed
; by Tauri's uninstaller. These hooks additionally remove an engine the app
; downloaded into app data, and ask whether to delete history and settings.
; The default is to KEEP user data. A silent uninstall (/S) never prompts; pass
; /DELETEDATA to delete history and settings. Files the user downloaded live in
; their own folders and are never touched.
!include "FileFunc.nsh"
!insertmacro un.GetParameters
!insertmacro un.GetOptions

!macro NSIS_HOOK_POSTUNINSTALL
  ; Engine downloaded by the app (managed copy).
  RMDir /r "$LOCALAPPDATA\strs.pencil.downloadmanager\engine"

  StrCpy $R8 "0"
  ${un.GetParameters} $R0
  ${un.GetOptions} $R0 "/DELETEDATA" $R1
  ${IfNot} ${Errors}
    StrCpy $R8 "1"
  ${EndIf}
  ClearErrors

  ${If} $R8 != "1"
  ${AndIfNot} ${Silent}
    MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "Also delete your download history and settings?$\r$\n$\r$\nYour downloaded files are not affected." IDYES +2
    Goto pdm_keep_data
    StrCpy $R8 "1"
  ${EndIf}

  ${If} $R8 == "1"
    RMDir /r "$LOCALAPPDATA\strs.pencil.downloadmanager"
  ${EndIf}
  pdm_keep_data:
!macroend
