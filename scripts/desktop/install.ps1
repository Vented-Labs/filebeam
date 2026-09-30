param([Parameter(Mandatory=$true)][string]$Installer)
if (!(Test-Path $Installer)) { throw 'Verified NSIS installer is required.' }
Start-Process -Wait -FilePath $Installer -ArgumentList '/S'
