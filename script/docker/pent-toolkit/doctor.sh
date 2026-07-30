#!/bin/sh
set -eu

failures=0

check() {
  if command -v "${1}" >/dev/null 2>&1; then
    echo "  ✓ ${1}"
  else
    echo "  ✗ missing required tool: ${1}" >&2
    failures=$((failures + 1))
  fi
}

echo "kairos pent toolkit health check"

# ── Core networking ──────────────────────────────────────────────────────
check nmap
check masscan
check naabu
check netdiscover
check nbtscan

# ── HTTP / Web clients ───────────────────────────────────────────────────
check curl
check wget

# ── DNS ──────────────────────────────────────────────────────────────────
check dig
check dnsrecon
check dnsenum
check dnsx
check fierce

# ── Web discovery / crawling ─────────────────────────────────────────────
check ffuf
check dirsearch
check gobuster
check feroxbuster
check dirb
check wfuzz
check hakrawler
check httrack
check lbd

# ── Web fingerprinting ───────────────────────────────────────────────────
check whatweb
check wafw00f

# ── Vulnerability scanners ───────────────────────────────────────────────
check nuclei
check nikto
check wpscan
check wapiti
check skipfish
check afrog

# ── Injection tools ──────────────────────────────────────────────────────
check sqlmap
check commix
check xsser
check dalfox
check arjun

# ── TLS / SSL ────────────────────────────────────────────────────────────
check testssl
check sslscan
check sslyze
check openssl

# ── Protocol / service enumeration ────────────────────────────────────────
check smbmap
check enum4linux
check smtp-user-enum
check swaks
check davtest

# ── OSINT / recon ────────────────────────────────────────────────────────
check whois
check dmitry
check theharvester
check amass
check subfinder

# ── SSH / SQL clients ────────────────────────────────────────────────────
check ssh
check psql

echo ""

if [ "${failures}" -gt 0 ]; then
  echo "health check FAILED: ${failures} tool(s) missing" >&2
  exit 1
fi

echo "kairos pent toolkit is healthy"
