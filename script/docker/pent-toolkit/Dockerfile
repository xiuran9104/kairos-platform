# ── Go tool builder ───────────────────────────────────────────────────────
# afrog + dalfox are not available in Kali apt — build from Go source.
FROM golang:1.24-bookworm AS go-tools

ARG AFROG_VERSION=v3.1.8
ARG DALFOX_VERSION=v2.9.3
RUN CGO_ENABLED=0 go install "github.com/zan8in/afrog/v3/cmd/afrog@${AFROG_VERSION}" \
    && CGO_ENABLED=0 go install "github.com/hahwul/dalfox/v2@${DALFOX_VERSION}"


# ── Runtime image ─────────────────────────────────────────────────────────
FROM kalilinux/kali-rolling

ARG DEBIAN_FRONTEND=noninteractive
ARG TOOLKIT_VERSION=kali-2026.1

LABEL org.opencontainers.image.title="Kairos Pent Toolkit" \
      org.opencontainers.image.description="Unified Kali Linux userspace with all Kairos Pent worker tools" \
      org.opencontainers.image.version="${TOOLKIT_VERSION}"

# ── Foundation + Kali headless toolset + pinned nuclei ───────────────────
# kali-linux-headless pulls in ~80% of the tools below. Explicit packages
# fill gaps and form the auditable contract matched by the Rust allow-list.
# Most projectdiscovery tools (subfinder/dnsx/naabu/nuclei/hakrawler) and
# dalfox are available in Kali apt — installing from apt avoids fragile
# per-tool Go version tracking.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        tini \
        # ── Kali headless meta (broad toolset) ──
        kali-linux-headless \
        # ── Explicit tool contract (Rust allow-list) ──
        amass \
        arjun \
        commix \
        curl \
        davtest \
        dirb \
        dirsearch \
        dmitry \
        dnsenum \
        dnsrecon \
        dnsutils \
        dnsx \
        enum4linux \
        feroxbuster \
        ffuf \
        fierce \
        gobuster \
        hakrawler \
        httrack \
        lbd \
        masscan \
        naabu \
        nbtscan \
        netdiscover \
        nikto \
        nmap \
        nuclei \
        openssh-client \
        openssl \
        postgresql-client \
        skipfish \
        smbmap \
        smtp-user-enum \
        sqlmap \
        sslscan \
        sslyze \
        subfinder \
        swaks \
        testssl.sh \
        theharvester \
        wafw00f \
        wapiti \
        wfuzz \
        whatweb \
        whois \
        wpscan \
        xsser \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# ── Go-built binary (only afrog — not in Kali repos) ─────────────────────
COPY --from=go-tools /go/bin/afrog  /usr/local/bin/afrog
COPY --from=go-tools /go/bin/dalfox /usr/local/bin/dalfox

# ── Toolkit metadata ─────────────────────────────────────────────────────
COPY script/docker/pent-toolkit/doctor.sh   /usr/local/bin/kairos-toolkit-doctor
COPY script/docker/pent-toolkit/manifest.json /opt/kairos-pent-toolkit/manifest.json

RUN chmod 0555 \
        /usr/local/bin/afrog \
        /usr/local/bin/dalfox \
        /usr/local/bin/kairos-toolkit-doctor \
    && chmod 0444 /opt/kairos-pent-toolkit/manifest.json \
    && /usr/local/bin/kairos-toolkit-doctor

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/kairos-toolkit-doctor"]
