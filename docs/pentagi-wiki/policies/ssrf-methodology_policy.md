---
name: ssrf-methodology_policy
description: PentAGI safety policy for Ssrf Methodology. 服务端请求伪造(SSRF)的检测与利用方法论。当目标有 URL 获取/预览/导入功能、PDF 生成、Webhook、远程图片加载、RSS 导入时使用。包含 IP 过滤绕过大全、协议利用(file/gopher/dict)、云元数据窃取、内网服务探测。即使参数名看起来无害（如 src、callback、next），只要涉及服务端发起请求的场景都应加载此 skill。
when-to-use: Use when Ssrf Methodology methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# SSRF Methodology — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/ssrf-methodology/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/ssrf-methodology/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/ssrf-methodology/SKILL.md`
