---
name: k8s-network-recon_policy
description: PentAGI safety policy for K8S Network Recon. Kubernetes 集群内网络侦察与服务发现。当已获得 Pod Shell、需要发现集群内其他服务、执行 K8s 内网扫描时使用。覆盖 DNS PTR 反查、SRV 记录枚举、AXFR 域传输、K8Spider 使用。任何在 Pod 中需要横向侦察、寻找隐藏服务、确定攻击目标的场景都应使用此技能，即使用户没有明确提到 DNS
when-to-use: Use when K8S Network Recon methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# K8S Network Recon — PentAGI Safety Policy

> Policy compiled from `cloud/k8s-network-recon/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `cloud/k8s-network-recon/SKILL.md` for knowledge reference
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

Source: `cloud/k8s-network-recon/SKILL.md`
