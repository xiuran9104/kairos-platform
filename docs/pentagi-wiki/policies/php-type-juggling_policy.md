---
name: php-type-juggling_policy
description: PentAGI safety policy for Php Type Juggling. PHP 类型杂耍（Type Juggling）和 Magic Hash 攻击方法论。当目标是 PHP 应用且存在密码比较、哈希校验、认证逻辑时使用。当看到 PHP `==` 松散比较、MD5/SHA1 哈希校验、strcmp 比较、JSON 输入处理、is_numeric 检查时必须使用。当错误登录泄露了输入的 MD5 哈希值时，这是 Magic Hash 的强信号，立即使用。CTF/渗透中 PH
when-to-use: Use when Php Type Juggling methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# PHP Type Juggling — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/php-type-juggling/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/php-type-juggling/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/php-type-juggling/SKILL.md`
