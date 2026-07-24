---
name: poc_review
description: Guidance for static review of exploit, PoC, template, script, or payload material when it may be relevant to a finding but execution is high risk, out of scope, or not yet approved.
when-to-use: Use when exploit or POC code appears relevant but execution would be high risk.
allowed-tools:
  - read_file
  - grep
  - bash
  - todo_write
---

# poc_review

There is no built-in exploit or POC executor. Keep POC handling as review and planning unless the user provides explicit authorization, target scope, and a separate execution path.

Use for:

- understanding exploit prerequisites and affected versions
- extracting safe detection indicators
- identifying network behavior and side effects
- converting exploit logic into non-destructive validation steps
- documenting why a PoC is unsafe or not applicable

Recommended flow:

1. Read the POC source statically.
2. Identify prerequisites, affected versions, network behavior, payloads, and side effects.
3. Convert the POC into a non-destructive validation plan when possible.
4. Use `bash` only for static local inspection commands such as syntax checks, archive listing, or grepping indicators.

Do not run exploit code, deploy shells, write webshells, modify target data, dump data, or chain post-exploitation actions from this skill.
