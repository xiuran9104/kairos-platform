---
name: api-fuzz_policy
description: PentAGI safety policy for Api Fuzz. REST/GraphQL API 安全测试方法论。当目标有 API 端点（/api/、JSON 响应）、Swagger/OpenAPI 文档暴露、通过 js-api-extract 或目录扫描获得端点列表时使用。覆盖 API 发现、认证测试、框架识别、语义分析智能 Fuzz（根据端点语义推断参数名/类型/业务含义构造精准 payload）、Prototype Pollution、请求走私。IDOR
when-to-use: Use when Api Fuzz methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# API Fuzz — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/api-fuzz/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/api-fuzz/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/api-fuzz/SKILL.md`
