---
name: pentagi-safe-tool-planning
description: Policy for planning PentAGI tasks around locally installed CLI tools, capability coverage, missing-tool suggestions, health checks, and approval-aware execution.
when-to-use: Use when deciding which CLI tools PentAGI should use.
tags: [pentagi, cli, tools, planner, coverage, env-doctor]
related: [policies/pentagi-authorized-pentest, tools/validation_dispatch, tools/vuln_hypothesis_plan]
---

# PentAGI Safe Tool Planning

Use this policy when deciding which CLI tools PentAGI should use.

## Planning Rules

- Discover installed tools and health before planning.
- Select tools by capability, not by a static hardcoded preference.
- Missing tools should produce an install plan, not automatic installation.
- Broken tools should be reported with diagnostic output.
- Commands must route through PentAGI policy checks.
- High-risk command patterns are denied even if a tool exists.

## Capability Groups

- Network discovery: `nmap`, `masscan`, `netdiscover`, `nbtscan`
- DNS/OSINT collection: `dnsrecon`, `dnsenum`, `fierce`, `dmitry`, `theHarvester`, `whois`, `amass`, `subfinder`, `dnsx`
- Service discovery: `smbmap`, `enum4linux`, `smtp-user-enum`, `swaks`, `naabu`
- Web crawling: `katana`, `gau`, `hakrawler`, `httrack`, `curl`
- Directory/parameter fuzzing: `ffuf`, `dirsearch`, `gobuster`, `feroxbuster`, `wfuzz`, `arjun`, `dirb`
- Web checks: `nikto`, `nuclei`, `whatweb`, `wafw00f`, `wpscan`, `wapiti`, `skipfish`, `davtest`, `lbd`
- TLS/SSL checks: `sslscan`, `sslyze`, `tlssled`, `qsslcaudit`
- Injection validation: `sqlmap`, `commix`, `xsser`
- API/static analysis: local JS/HTML parsers, `curl`
- Reporting: PentAGI report and Wiki update modules

## Tool Details

### Network Discovery

- `nmap`: General port scanning, service identification, and basic script probing for authorized service inventory.
- `masscan`: High-speed broad port discovery for large authorized ranges; results should be reviewed with `nmap`.
- `netdiscover`: LAN ARP host discovery for internal or lab networks.
- `nbtscan`: NetBIOS name and host information enumeration for Windows/SMB networks.

### DNS/OSINT Collection

- `dnsrecon`: DNS record enumeration, zone transfer checks, and reverse lookup support.
- `dnsenum`: DNS NS/MX/A/AAAA, subdomain, and reverse lookup enumeration.
- `fierce`: DNS subdomain and adjacent address discovery.
- `dmitry`: Whois, subdomain, email, and port aggregation.
- `theHarvester`: Public-source email, subdomain, hostname, and people collection.
- `whois`: Domain and IP registration lookup.
- `amass`: Larger subdomain discovery and asset graphing.
- `subfinder`: Passive subdomain discovery.
- `dnsx`: Bulk DNS resolution, filtering, and validation.

### Service Discovery

- `smbmap`: SMB share enumeration and permission overview.
- `enum4linux`: Windows/Samba domain, user, group, share, and policy enumeration.
- `smtp-user-enum`: SMTP user enumeration only when explicitly authorized.
- `swaks`: SMTP protocol testing for configuration, authentication, and delivery behavior.
- `naabu`: Lightweight port discovery before deeper `nmap` service identification.

### Web Crawling

- `katana`: Modern crawler for URL, endpoint, and frontend asset discovery.
- `gau`: Historical URL collection from archives and public sources.
- `hakrawler`: Lightweight site link and JS/CSS resource crawling.
- `httrack`: Authorized offline static site mirroring.
- `curl`: HTTP response, header, auth, redirect, and API behavior validation.

### Directory/Parameter Fuzzing

- `ffuf`: High-performance directory, file, parameter, virtual host, and endpoint discovery.
- `dirsearch`: Conventional directory and file discovery.
- `gobuster`: Directory, DNS, and virtual host discovery.
- `feroxbuster`: Recursive content discovery.
- `wfuzz`: Custom fuzzing for parameters, headers, paths, and forms.
- `arjun`: HTTP parameter name discovery.
- `dirb`: Classic baseline directory scanning.

### Web Checks

- `nikto`: Web server misconfiguration and known issue checks.
- `nuclei`: Template-based known risk checks within authorized scope.
- `whatweb`: Framework, CMS, middleware, and technology fingerprinting.
- `wafw00f`: WAF/CDN identification.
- `wpscan`: Authorized WordPress version, plugin, theme, and vulnerability checks.
- `wapiti`: Basic web risk checks such as injection, XSS, and file inclusion.
- `skipfish`: Supplemental site structure and common risk discovery.
- `davtest`: WebDAV method and configuration validation.
- `lbd`: Load balancer detection.

### TLS/SSL Checks

- `sslscan`: TLS/SSL protocol, cipher, and certificate checks.
- `sslyze`: Deeper TLS certificate and handshake configuration checks.
- `tlssled`: Quick TLS/SSL baseline checks.
- `qsslcaudit`: Supplemental TLS trust and certificate configuration checks.

### Injection Validation

- `sqlmap`: SQL injection validation and database type identification only for authorized targets and explicit test points.
- `commix`: Command injection validation only within authorized scope.
- `xsser`: XSS validation support for suspected reflected or stored XSS.

### API/Static Analysis

- local JS/HTML parsers: Extract routes, APIs, source maps, hardcoded config, and frontend authorization logic from local files.
- `curl`: Minimal API reproduction with status, header, body, and auth behavior capture.

### Reporting

- PentAGI report modules: Summarize evidence, reproduction steps, impact, remediation, and tool output.
- PentAGI Wiki update modules: Write stable methods, target lessons, and validation results back to LLMWiki.

Kali Web tools that perform brute force or post-exploitation are manual-only by default. Do not include them in automated plans unless authorization explicitly allows them.

## Denied Automation

Do not plan:

- brute force
- WAF bypass loops
- reverse or bind shells
- database dumps
- file reads or writes through exploit tools
- persistence
- privilege escalation
- lateral movement

## Preferred PentAGI Functions

- `pentagi_env_doctor`
- `pentagi_capability_coverage`
- `pentagi_tool_install_plan`
- `pentagi_tool_install_log`
