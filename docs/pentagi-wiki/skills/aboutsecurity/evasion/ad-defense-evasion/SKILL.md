---
name: ad-defense-evasion
description: "AD环境防御绕过(AMSI/ETW/AppLocker/PowerShell CLM)。仅当用户明确要求在AD域环境中绕过Windows防御机制、执行受保护的脚本或工具时触发。不自动触发"
risk: restricted
requires-explicit-authorization: true
metadata:
  tags: "AMSI, ETW, AppLocker, CLM,防御绕过, PowerShell, Constrained Language Mode, bypass, evasion"
  category: "evasion"
---

# AD 环境防御绕过

> ⚠️ 仅在用户明确要求绕过 AD 环境中的 Windows 防御机制时使用。

## 深入参考

- AMSI 绕过技术 → [references/amsi-bypass.md](references/amsi-bypass.md)
- ETW (Event Tracing for Windows) 绕过 → [references/etw-bypass.md](references/etw-bypass.md)
- AppLocker / WDAC 绕过 → [references/applocker-bypass.md](references/applocker-bypass.md)
- PowerShell CLM 绕过 → [references/powershell-clm-bypass.md](references/powershell-clm-bypass.md)

---

## 防御矩阵速查

| 防御 | 触发条件 | 影响 |
|------|---------|------|
| AMSI | PowerShell/VBS/JS/.NET 执行时 | 阻止已知恶意脚本 |
| ETW | .NET加载/进程创建/网络连接 | 记录日志(EDR数据源) |
| AppLocker | 非白名单路径执行exe/dll/脚本 | 阻止执行 |
| CLM | PowerShell 启动时 | 限制为 Constrained Language 模式 |

## 防御优先级

```
1. 先绕过 AMSI → 否则所有恶意脚本都会被拦截
2. 再绕过 ETW → 否则 EDR 会记录所有操作
3. 如果需要执行自定义 exe/dll → 绕过 AppLocker
4. 如果需要完整 PowerShell 功能 → 绕过 CLM
```
