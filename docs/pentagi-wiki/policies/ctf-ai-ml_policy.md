---
name: ctf-ai-ml_policy
description: PentAGI safety policy for CTF AI Ml. CTF AI/ML 攻击技术。当挑战涉及 AI 模型攻击、对抗样本生成、模型提取、Prompt 注入/越狱、LoRA 权重操纵、LLM Token 走私、成员推理攻击、训练数据投毒、神经网络分析时使用。覆盖 FGSM/PGD/C&W 对抗攻击、模型反演、模型权重扰动还原、LLM 工具链劫持、上下文窗口操纵等 AI 安全全链路攻防技术
when-to-use: Use when CTF AI Ml methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - todo_write
---

# CTF AI Ml — PentAGI Safety Policy

> Policy compiled from `ctf/ctf-ai-ml/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ctf/ctf-ai-ml/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous

## Blocked Operations

- 对非 CTF 目标使用
- 破坏 CTF 基础设施
- 泄露 flag 给未授权方

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

Source: `ctf/ctf-ai-ml/SKILL.md`
