$ErrorActionPreference = 'Stop'
$desktop = [Environment]::GetFolderPath('Desktop')
$projectDir = 'D:\api-monitor'
$batPath = Join-Path $projectDir '启动测试.bat'
$lnkPath = Join-Path $desktop 'API Monitor 测试.lnk'

$ws = New-Object -ComObject WScript.Shell
$sc = $ws.CreateShortcut($lnkPath)
$sc.TargetPath = $batPath
$sc.WorkingDirectory = $projectDir
$sc.Description = '启动 API Monitor 测试模式 (npm run tauri dev)'
$sc.Save()

Write-Output "快捷方式已创建: $lnkPath"
