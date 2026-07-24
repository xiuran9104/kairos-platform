---
name: prompt-leak_policy
description: PentAGI safety policy for Prompt Leak. AI/LLM 系统提示词泄露技术。当目标是基于 LLM 的应用且想获取其系统提示词、配置信息、工具列表、知识库内容时使用。AI Bug Bounty、红队评估、竞品分析的关键第一步
when-to-use: Use when Prompt Leak methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# Prompt Leak — PentAGI Safety Policy

> Policy compiled from `ai-security/prompt-leak/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ai-security/prompt-leak/SKILL.md` for knowledge reference
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

Source: `ai-security/prompt-leak/SKILL.md`
