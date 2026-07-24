---
name: dns_lookup
description: Guidance for DNS triage with the built-in dns_lookup tool and optional resolver checks when domain ownership, host mapping, or DNS records guide authorized target discovery.
when-to-use: Use when domain ownership, host mapping, or DNS records guide target discovery.
allowed-tools:
  - dns_lookup
  - command_check
  - bash
  - todo_write
---

# dns_lookup

Use `dns_lookup` first for built-in A/AAAA resolution through the system resolver. Request record types explicitly when documenting intent.

Use for:

- resolving authorized domains to hosts
- checking basic host mapping before HTTP probing
- documenting DNS evidence in target notes
- deciding whether additional passive recon is needed

Planning notes from AboutSecurity raw material:

- For larger domain inventories, separate passive discovery, DNS resolution, wildcard filtering, and HTTP probing into distinct steps.
- Record query type and resolver source when DNS evidence affects scope or routing.
- Treat CNAME, NS, MX, and TXT records as context for ownership, mail surface, SaaS dependencies, and routing, not as findings by themselves.
- If subdomain lists are supplied from passive recon, validate them conservatively and filter wildcard responses before follow-up probing.

For CNAME, MX, TXT, or NS records, `dns_lookup` reports the requested types but the current built-in backend does not resolve them directly. If needed, use `command_check` for `dig` or `nslookup`, then run a bounded resolver command through `bash`.

Recommended flow:

1. Resolve A/AAAA with `dns_lookup`.
2. If other records are needed, check for `dig` or `nslookup`.
3. Record DNS answers with timestamps and resolver source.
4. Feed resulting hosts back into `http_probe` or `nmap_scan` only if in scope.

Source references: compiled from `wiki/skills/tool/dnsx-resolve/SKILL.md` and `wiki/skills/tool/subfinder-recon/SKILL.md`; high-volume brute-force enumeration is excluded.
