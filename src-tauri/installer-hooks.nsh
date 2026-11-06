; An install whose files were deleted by hand, without its uninstaller, still
; has its keys in the registry. The generated script then offers "Uninstall
; before installing", which runs an uninstaller that isn't there and ends in
; "Unable to uninstall" (PageReinstall and PageLeaveReinstall, installer.nsi
; lines 188 to 381 at tauri-cli-v2.11.4, read 6 November 2026). So before
; that page, an install that is gone is forgotten: when neither its
; uninstaller nor its main program is where its keys say, the keys an
; install writes (lines 682 to 718) are removed as the uninstaller removes
; them (lines 853 to 879). The page then finds nothing (lines 217 to 220),
; the installer goes on as a fresh install, and Windows' list of installed
; apps loses the dead entry. No file or folder is touched. An install still
; there, Omoio's own updates (/UPDATE) and a silent install go as before.
;
; Modern UI 2 runs MUI_CUSTOMFUNCTION_GUIINIT from its own .onGUIInit, just
; before the first page shows, and the define has to come before the
; language macros (NSIS Modern UI 2 readme, "General Custom Functions"),
; which this file does: it is included at line 35, the languages at line
; 470, and the template sets no GUIINIT of its own. .onInit has already run
; by then, so SHCTX is this user's part of the registry (line 497).
;
; This file is included before the template defines its key names (lines 40
; to 68), so they are spelled out here, from productName and publisher in
; tauri.conf.json, and NSIS_HOOK_PREINSTALL stops the build if they ever
; differ from the template's.

!define MUI_CUSTOMFUNCTION_GUIINIT ForgetGoneInstall
; A test script names keys of its own first, so it never touches Omoio's.
!ifndef OMOIO_UNINSTKEY
  !define OMOIO_UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Omoio"
  !define OMOIO_MANUKEY "Software\Omoio"
  !define OMOIO_MANUPRODUCTKEY "Software\Omoio\Omoio"
!endif

Function ForgetGoneInstall
  ${If} ${Silent}
    Return
  ${EndIf}
  Push $0
  Push $1
  Push $2
  ClearErrors
  ${GetOptions} $CMDLINE "/UPDATE" $0
  ${If} ${Errors}
    ReadRegStr $0 SHCTX "${OMOIO_UNINSTKEY}" "UninstallString"
    ReadRegStr $1 SHCTX "${OMOIO_UNINSTKEY}" "InstallLocation"
    ReadRegStr $2 SHCTX "${OMOIO_UNINSTKEY}" "MainBinaryName"
    ; Both paths are written in quotes (lines 702 and 703).
    ${WordReplace} $0 '"' "" "+" $0
    ${WordReplace} $1 '"' "" "+" $1
    ${If} $0 != ""
    ${AndIf} $1 != ""
    ${AndIf} $2 != ""
    ${AndIfNot} ${FileExists} "$0"
    ${AndIfNot} ${FileExists} "$1\$2"
      DeleteRegKey SHCTX "${OMOIO_UNINSTKEY}"
      DeleteRegKey SHCTX "${OMOIO_MANUPRODUCTKEY}"
      DeleteRegKey /ifempty SHCTX "${OMOIO_MANUKEY}"
    ${EndIf}
  ${EndIf}
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
  !if "${OMOIO_UNINSTKEY}${OMOIO_MANUKEY}${OMOIO_MANUPRODUCTKEY}" != "${UNINSTKEY}${MANUKEY}${MANUPRODUCTKEY}"
    !error "installer-hooks.nsh names other registry keys than the installer writes"
  !endif
!macroend

; Uninstall behaviour, which the generated script gets wrong for this app in
; two ways.
;
; First, it deletes the executable and then calls RMDir without /r, which only
; removes an empty folder. Omoio keeps the emulators it downloaded inside its
; own install folder, so that call fails quietly and leaves them behind.
;
; Second, the "delete application data" checkbox removes $APPDATA\<bundle id>,
; and this app stores nothing there. Its library, save backups, logs and cover
; art live in $APPDATA\Omoio, so ticking the box removed nothing at all while
; telling the user otherwise.
;
; The emulators hold the games' own saves (RPCS3's dev_hdd0, Cemu's
; portable\mlc01) and the user's Wii U keys, so they go only with the user's
; other things, when the checkbox that already exists is ticked. Left unticked,
; they stay for a reinstall to find. Their games are never touched: those live
; wherever they put them, and Omoio only ever referenced them.

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    SetShellVarContext current

    ${If} $DeleteAppDataCheckboxState = 1
      RMDir /r "$INSTDIR"
      RMDir /r "$APPDATA\Omoio"
    ${EndIf}
  ${EndIf}
!macroend
