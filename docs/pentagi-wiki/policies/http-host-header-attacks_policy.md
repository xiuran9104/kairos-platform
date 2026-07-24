---
name: http-host-header-attacks_policy
description: PentAGI safety policy for Http Host Header Attacks. HTTP Host Header 攻击方法论。当目标存在密码重置、缓存机制、反向代理、虚拟主机、重定向功能时使用。覆盖密码重置投毒(Host注入窃取reset token)、Web缓存投毒(Host控制缓存键)、通过Host路由SSRF、虚拟主机枚举与跨站读取、绕过技术(X-Forwarded-Host/双Host/绝对URI/端口注入/换行符注入)。任何涉及密码重置、Host头处理、缓存、虚拟主
when-to-use: Use when Http Host Header Attacks methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Http Host Header Attacks — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/http-host-header-attacks/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/http-host-header-attacks/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 无 confirm_authorized 的主动利用
- 破坏性 payload（DROP/DELETE/rm -rf）
- 直接执行 OS 命令
- 写入 webshell 或后门文件

## Escalation Path

If the methodology requires blocked operations:
1. Stop automated execution
2. Document the specific need and target
3. Request explicit human authorization with scope boundaries
4. Wait for approval before proceeding

## Evidence Requirements

- Record every probe attempt with request/response evidence
- Link findings to the source methodology path
- Mark unverified observations as `needs_review`

---

Source: `exploit/web-method/http-host-header-attacks/SKILL.md`
