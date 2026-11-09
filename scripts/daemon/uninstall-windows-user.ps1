$task = Get-ScheduledTask -TaskName 'Filebeam Daemon' -TaskPath '\Filebeam\' -ErrorAction SilentlyContinue
if ($task) { Unregister-ScheduledTask -TaskName 'Filebeam Daemon' -TaskPath '\Filebeam\' -Confirm:$false }
