---
name: serverless-attack_policy
description: PentAGI safety policy for Serverless Attack. Serverless/云函数安全测试与攻击。当目标涉及 AWS Lambda、腾讯云 SCF、阿里云 FC、Azure Functions 等 Serverless 服务时使用。当发现 API Gateway 后端是 Lambda/SCF 触发、通过 cloud-aksk-exploit 获取到函数操作权限、或需要分析云函数代码中的漏洞时使用。覆盖事件注入（HTTP/OSS/消息队列触发器参数篡改
when-to-use: Use when Serverless Attack methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# Serverless Attack — PentAGI Safety Policy

> Policy compiled from `cloud/serverless-attack/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `cloud/serverless-attack/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 创建后门/持久化资源
- 跨账户横向移动
- 修改 IAM/CAM 策略
- 删除 CloudTrail/审计日志

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

Source: `cloud/serverless-attack/SKILL.md`
