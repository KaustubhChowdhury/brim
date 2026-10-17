; Brim NSIS installer hooks.
; The "launch at sign-in" option writes a per-user Run entry. Remove it on uninstall so no
; dangling startup entry is left behind. User data removal is controlled by the uninstaller's
; "Delete the application data" checkbox (unchecked by default = data is kept).
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Brim"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Brim"
  ; Entry name used before the rename from "Wisp Dock".
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Wisp Dock"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Wisp Dock"
!macroend
