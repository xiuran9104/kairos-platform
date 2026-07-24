---
name: web-vuln-scan_policy
description: PentAGI safety policy for Web Vuln Scan. 单目标 Web 深度漏洞扫描策略。当已知目标 URL、需要对一个 Web 应用进行深度漏洞测试时使用。与 recon-full（资产发现）不同——本技能假设目标已确定，聚焦漏洞层面的深度测试。覆盖指纹驱动的扫描策略选择、自动化+手动测试结合、漏洞优先级排序。优先发现 RCE、SQL注入等高危漏洞
when-to-use: Use when Web Vuln Scan methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Web Vuln Scan — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/web-vuln-scan/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/web-vuln-scan/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/web-vuln-scan/SKILL.md`
