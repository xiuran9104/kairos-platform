# file_upload_validate

Safely validates an authorized file-upload finding with evidence collection.

Use this tool when a file upload issue needs proof beyond scanner output. The tool can upload marker content using a dangerous extension that matches the target stack or scoring condition and optionally fetch a `verify_url` to confirm storage or impact signals.

Important semantics:

- `storage_confirmed=true` means the file was accepted or retrieved. This alone is not a vulnerability.
- `validated=true` only when `impact_validated=true`.
- Plain PNG, TXT, or normal business uploads do not validate impact.
- Use marker filenames with the target-relevant parser extension, such as `.php`, `.phtml`, `.jsp`, `.jspx`, `.asp`, `.aspx`, `.ashx`, `.cfm`, `.py`, `.wsgi`, `.rb`, `.erb`, `.cgi`, `.pl`, `.lua`, `.js`, or another extension required by the target stack.
- Inert server-side parser marker output is allowed when it only prints a fixed marker and does not read request parameters, execute commands, read/write files, open network connections, or persist access.
- Webshells, request-driven behavior, real archives, executable bodies, shell scripts, PowerShell, command execution payloads, file/network access payloads, backdoors, and WAF bypass mutation are denied.
- Dangerous-extension impact is only validated when `execution_marker` appears in the `verify_url` response for the uploaded marker file.

Useful evidence:

- upload request and response
- retrieval request and response
- response Content-Type
- marker or execution_marker presence
- impact_signals
