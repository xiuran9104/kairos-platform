---
name: ctf-crypto_policy
description: PentAGI safety policy for CTF Crypto. CTF 密码学攻击技术。用于 RSA/AES/ECC/格密码/PRNG/ZKP/古典密码等 CTF 加密类挑战。当遇到加密数据需要破解、密码学相关 CTF 题目、需要分析加密算法弱点、或识别到密文/公钥/密码学参数时使用。覆盖从古典替换密码到现代公钥密码、椭圆曲线、格攻击、零知识证明等全方位密码学攻防技术
when-to-use: Use when CTF Crypto methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - todo_write
---

# CTF Crypto — PentAGI Safety Policy

> Policy compiled from `ctf/ctf-crypto/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `ctf/ctf-crypto/SKILL.md` for knowledge reference
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

Source: `ctf/ctf-crypto/SKILL.md`
