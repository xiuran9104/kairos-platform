---
name: nmap_scan
description: Guidance for bounded TCP port and service discovery with the nmap_scan wrapper after scope is explicit and service inventory is needed before web, TLS, or protocol-specific follow-up.
when-to-use: Use after scope is confirmed and the next step is TCP port/service identification.
allowed-tools:
  - command_check
  - nmap_scan
  - http_probe
  - dns_lookup
  - todo_write
---

# nmap_scan

Use `command_check` for `nmap` before scanning. Prefer `nmap_scan` over raw `bash` because it limits arguments and output.

Use for:

- initial TCP service inventory on authorized hosts
- identifying HTTP(S), SSH, SMB, database, and admin surfaces
- routing open ports to safer protocol-specific tools
- capturing reproducible service evidence

Planning notes from AboutSecurity raw material:

- Use `safe_default` first when the target state is unknown; escalate only to narrower, reviewed follow-up checks.
- Prefer explicit `ports` when another source already identified likely services. Broad scans should be a conscious scope decision, not a default.
- Treat service/version and script-like findings as routing hints until validated with protocol-specific tools and evidence.
- Use output to build a service inventory: host, port, protocol, product/version when present, and next PentAGI tool.

Recommended flow:

1. Confirm target is explicitly in scope.
2. Run `nmap_scan` with `profile: "safe_default"` for initial service discovery.
3. Use `ports` only for a known comma/range list such as `80,443,8000-8100`.
4. Inspect XML output, identify open services, then branch to `http_probe`, `tls_inspect`, or service-specific analysis.

Source reference: compiled from `wiki/skills/tool/nmap-scan/SKILL.md` with high-risk scan modes excluded.

Do not use this skill for UDP sweeps, OS fingerprinting, NSE vuln scripts, decoys, spoofing, packet fragmentation, or high-rate scans. If those are needed, stop and ask for explicit authorization and a separate reviewed workflow.
