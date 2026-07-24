---
name: prompt-injection_policy
description: PentAGI safety policy for Prompt Injection. AI/LLM 间接 Prompt 注入攻击。当目标 AI 系统会处理外部数据源（网页、文档、邮件、数据库、API 返回）时使用。覆盖间接注入、工具链劫持、RAG 投毒、数据外泄等技术。OWASP LLM Top 10 #1 漏洞类别
when-to-use: Use when Prompt Injection methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# Prompt Injection — PentAGI Safety Policy

> Policy compiled from `ai-security/prompt-injection/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ai-security/prompt-injection/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 提取训练数据/模型权重
- 修改模型参数
- 对生产模型执行对抗攻击

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

Source: `ai-security/prompt-injection/SKILL.md`
