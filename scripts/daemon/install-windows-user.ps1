param(
    [Parameter(Mandatory=$true)][string]$Daemon,
    [string]$Home = $(if ($env:FILEBEAM_HOME) { $env:FILEBEAM_HOME } else { Join-Path $HOME '.filebeam' })
)

throw 'Windows daemon installation is unavailable: filebeamd does not yet implement a per-user DACL-authenticated named pipe. No scheduled task was created.'
