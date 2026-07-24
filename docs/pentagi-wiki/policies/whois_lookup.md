---
name: whois_lookup
description: Guidance for passive, bounded whois lookups for domains, IPs, or ASNs when registration, organization, abuse contact, ASN, or netblock context is needed before active testing.
when-to-use: Use for passive ownership and registration context before active testing.
allowed-tools:
  - command_check
  - whois_lookup
  - dns_lookup
  - todo_write
---

# whois_lookup

Use `whois_lookup` for passive registration and ownership context. Check `whois` availability with `command_check` if execution fails.

Use for:

- ownership and registrar context
- ASN/net range notes
- report evidence about target attribution
- scope clarification with humans

Recommended flow:

1. Run `whois_lookup` for the domain, IP, or ASN.
2. Extract registrar, org, abuse contacts, net range, and relevant dates.
3. Use findings only to refine scope and evidence notes.

Do not expand testing scope based solely on whois adjacency. Scope still controls execution.
