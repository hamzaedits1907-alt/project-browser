; Project Browser: Windows default-browser registration (per-user install, so everything lives under HKCU).
;
; This makes Windows list Project Browser in Settings > Apps > Default apps and in "Open with". It never changes
; which browser is the default: only the user can choose that in Windows Settings (the UserChoice keys are not touched).
;
;   ProjectBrowserURL   ProgID for http and https links
;   ProjectBrowserHTM   ProgID for .htm/.html/.xhtml/.shtml/.svg files
;   Software\Clients\StartMenuInternet\Project Browser   the browser and its Capabilities
;   Software\RegisteredApplications   "Project Browser" -> the Capabilities key

!define PB_CLIENT "Software\Clients\StartMenuInternet\Project Browser"
!define PB_CAPS "${PB_CLIENT}\Capabilities"

!macro PB_PROGID KEY DESC
  WriteRegStr HKCU "Software\Classes\${KEY}" "" "${DESC}"
  WriteRegStr HKCU "Software\Classes\${KEY}" "FriendlyTypeName" "${DESC}"
  WriteRegStr HKCU "Software\Classes\${KEY}\Application" "ApplicationName" "Project Browser"
  WriteRegStr HKCU "Software\Classes\${KEY}\Application" "ApplicationIcon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\${KEY}\Application" "ApplicationDescription" "Gaming-first web browser"
  WriteRegStr HKCU "Software\Classes\${KEY}\Application" "ApplicationCompany" "Project Browser"
  WriteRegStr HKCU "Software\Classes\${KEY}\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\${KEY}\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" --single-argument %1'
!macroend

!macro PB_FILE_ASSOC EXT
  WriteRegStr HKCU "${PB_CAPS}\FileAssociations" "${EXT}" "ProjectBrowserHTM"
  WriteRegStr HKCU "Software\Classes\${EXT}\OpenWithProgids" "ProjectBrowserHTM" ""
!macroend

!macro PB_FILE_UNASSOC EXT
  DeleteRegValue HKCU "Software\Classes\${EXT}\OpenWithProgids" "ProjectBrowserHTM"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro PB_PROGID "ProjectBrowserURL" "Project Browser URL"
  WriteRegStr HKCU "Software\Classes\ProjectBrowserURL" "URL Protocol" ""
  !insertmacro PB_PROGID "ProjectBrowserHTM" "Project Browser HTML Document"

  WriteRegStr HKCU "${PB_CLIENT}" "" "Project Browser"
  WriteRegStr HKCU "${PB_CLIENT}\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "${PB_CLIENT}\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe"'
  WriteRegStr HKCU "${PB_CAPS}" "ApplicationName" "Project Browser"
  WriteRegStr HKCU "${PB_CAPS}" "ApplicationIcon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "${PB_CAPS}" "ApplicationDescription" "Gaming-first web browser"
  WriteRegStr HKCU "${PB_CAPS}\StartMenu" "StartMenuInternet" "Project Browser"
  WriteRegStr HKCU "${PB_CAPS}\URLAssociations" "http" "ProjectBrowserURL"
  WriteRegStr HKCU "${PB_CAPS}\URLAssociations" "https" "ProjectBrowserURL"
  !insertmacro PB_FILE_ASSOC ".htm"
  !insertmacro PB_FILE_ASSOC ".html"
  !insertmacro PB_FILE_ASSOC ".shtml"
  !insertmacro PB_FILE_ASSOC ".xht"
  !insertmacro PB_FILE_ASSOC ".xhtml"
  !insertmacro PB_FILE_ASSOC ".svg"
  WriteRegStr HKCU "Software\RegisteredApplications" "Project Browser" "${PB_CAPS}"

  ; Tell Explorer and Settings that the associations changed (SHCNE_ASSOCCHANGED, SHCNF_IDLIST).
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\RegisteredApplications" "Project Browser"
  DeleteRegKey HKCU "${PB_CLIENT}"
  DeleteRegKey HKCU "Software\Classes\ProjectBrowserURL"
  DeleteRegKey HKCU "Software\Classes\ProjectBrowserHTM"
  !insertmacro PB_FILE_UNASSOC ".htm"
  !insertmacro PB_FILE_UNASSOC ".html"
  !insertmacro PB_FILE_UNASSOC ".shtml"
  !insertmacro PB_FILE_UNASSOC ".xht"
  !insertmacro PB_FILE_UNASSOC ".xhtml"
  !insertmacro PB_FILE_UNASSOC ".svg"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend
