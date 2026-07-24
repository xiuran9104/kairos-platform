---
name: xxe-injection-methodology_policy
description: PentAGI safety policy for Xxe Injection Methodology. XML外部实体注入(XXE)的检测与利用方法论。当目标有 XML 解析、SOAP API、文件上传（DOCX/XLSX/SVG）、或任何接受 XML 输入的端点时使用。包含基础文件读取、盲 XXE 外带（参数实体+外部DTD）、SVG/DOCX XXE、SOAP Envelope XXE、JSON→XML 转换攻击。即使 API 文档说只接受 JSON，也应尝试 XML Content-Type 
when-to-use: Use when Xxe Injection Methodology methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# XXE Injection Methodology — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/xxe-injection-methodology/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/xxe-injection-methodology/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/xxe-injection-methodology/SKILL.md`
