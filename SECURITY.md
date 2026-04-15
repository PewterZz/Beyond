# Security Policy

## Reporting a Vulnerability

**Please do not open public GitHub issues for security vulnerabilities.**

Instead, report them privately via [GitHub Security Advisories](https://github.com/PewterZz/Beyond/security/advisories/new). You will receive an acknowledgement within a reasonable timeframe and we will work with you to investigate and disclose responsibly.

When reporting, please include:

- A description of the vulnerability and its impact
- Steps to reproduce (a minimal proof-of-concept is ideal)
- The affected version / commit
- Your suggested remediation, if any

## Supported Versions

Beyond is pre-1.0. Only the `main` branch is actively maintained. Security fixes will land on `main` and be announced in release notes.

## Scope

In scope:

- Sandbox escapes or privilege escalation via the agent runtime or `CapabilityBroker`
- Arbitrary code execution via crafted agent output, tool responses, or PTY escape sequences
- Path traversal, SSRF, or command injection in tool implementations
- Credential leakage (Ollama Turbo keys, session tokens) in logs, UI, or persisted blocks

Out of scope:

- Vulnerabilities requiring a locally compromised user account or root
- Bugs in upstream crates (please report those upstream)
- Denial-of-service via obviously abusive local input
