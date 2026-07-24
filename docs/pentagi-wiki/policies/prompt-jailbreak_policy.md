---
name: prompt-jailbreak_policy
description: PentAGI safety policy for Prompt Jailbreak. AI/LLM Prompt 越狱技术。当目标系统包含 AI 聊天机器人、AI Agent 或任何基于 LLM 的对话系统时使用。覆盖角色扮演、编码绕过、多语言切换、上下文窗口攻击、逻辑链递进、系统提示覆盖等越狱方法
when-to-use: Use when Prompt Jailbreak methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# Prompt Jailbreak — PentAGI Safety Policy

> Policy compiled from `ai-security/prompt-jailbreak/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ai-security/prompt-jailbreak/SKILL.md` for knowledge reference
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

Source: `ai-security/prompt-jailbreak/SKILL.md`
