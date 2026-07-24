---
name: xslt-injection_policy
description: PentAGI safety policy for Xslt Injection. XSLT 注入测试方法论。当目标存在用户可控的 XSLT/样式表输入、XML 转换端点、报表生成器、XML→HTML 转换器时使用。覆盖处理器指纹识别(Xalan/Saxon/libxslt/MSXML)、XXE via XSLT(DTD外部实体)、document() SSRF/文件读取、EXSLT 文件写入、PHP php:function RCE、Java Saxon/Xalan 扩展 RC
when-to-use: Use when Xslt Injection methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# XSLT Injection — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/xslt-injection/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/xslt-injection/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/xslt-injection/SKILL.md`
