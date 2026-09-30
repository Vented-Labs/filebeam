Name "Filebeam"
OutFile "${OUTPUT}"
Icon "${ICON}"
UninstallIcon "${ICON}"
InstallDir "$PROFILE\.filebeam"
RequestExecutionLevel user
Section
SetOutPath "$INSTDIR\bin"
File /oname=filebeam.exe "${INPUT}"
WriteUninstaller "$INSTDIR\uninstall.exe"
WriteRegStr HKCU "Software\Classes\filebeam" "" "URL:Filebeam Protocol"
WriteRegStr HKCU "Software\Classes\filebeam" "URL Protocol" ""
WriteRegStr HKCU "Software\Classes\filebeam\shell\open\command" "" '"$INSTDIR\bin\filebeam.exe" "%1"'
CreateShortCut "$SMPROGRAMS\Filebeam.lnk" "$INSTDIR\bin\filebeam.exe"
SectionEnd
Section Uninstall
DeleteRegKey HKCU "Software\Classes\filebeam"
Delete "$SMPROGRAMS\Filebeam.lnk"
Delete "$INSTDIR\bin\filebeam.exe"
Delete "$INSTDIR\uninstall.exe"
RMDir "$INSTDIR\bin"
RMDir "$INSTDIR"
SectionEnd
