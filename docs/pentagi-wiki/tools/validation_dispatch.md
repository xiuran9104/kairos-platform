# validation_dispatch

Converts findings and validation plans into concrete next tool calls.

This tool does not execute validation itself. It prepares safe calls and blocked reasons based on:

- `validation_kind`
- `confirm_authorized`
- `test_accounts_confirmed`
- target URL validity
- tool-specific guardrails

For file upload findings, it emits a baseline `http_request` and a `file_upload_validate` call. The generated upload call uses harmless dangerous-extension marker content by default and should be adjusted with the correct field name, target-relevant extension such as `.php`/`.jsp`/`.jspx`/`.asp`/`.aspx`/`.war`/`.jar`/`.exe`/`.sh`/`.ps1`, and verify URL before execution.

When WAF/CDN/security-control behavior is observed, run `waf_observation_plan` before selecting additional validators. WAF raw knowledge is for evidence classification and manual review only; do not generate automated payload mutation tables or bypass loops.

Blocked output is expected when authorization, test accounts, or target context are missing.
