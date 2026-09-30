Name "Filebeam"
OutFile "${OUTPUT}"
Icon "${ICON}"
UninstallIcon "${ICON}"
InstallDir "$LOCALAPPDATA\Filebeam"
RequestExecutionLevel user
Section
SetOutPath "$INSTDIR\bin"
File "${INPUT}"
WriteUninstaller "$INSTDIR\uninstall.exe"
WriteRegStr HKCU "Software\Classes\filebeam\shell\open\command" "" '"$INSTDIR\bin\filebeam.exe" "%1"'
SectionEnd
Section Uninstall
DeleteRegKey HKCU "Software\Classes\filebeam"
Delete "$INSTDIR\bin\filebeam.exe"
Delete "$INSTDIR\uninstall.exe"
RMDir "$INSTDIR\bin"
RMDir "$INSTDIR"
SectionEnd
