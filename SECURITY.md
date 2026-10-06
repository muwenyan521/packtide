English | [简体中文](SECURITY.zh-CN.md)

# Security

These tools start package managers and can run privileged system transactions. Before confirming a write, check the backend, scope, and transaction summary. Build inputs, release archives, and external package-manager executables should come from sources you trust.

## Reporting

Do not put exploit details, credentials, or personal data in a public issue. Use the host's private vulnerability channel when one is available; otherwise ask the maintainer for a private contact. This repository does not promise a response time or a supported-version security window.

Include the affected commit/version, backend and scope, reproduction in a disposable environment, expected and observed behavior, and a redacted command/output capture. Do not run a proof of concept on another user's system or the host package database.

## Boundaries

Reads run as the invoking user. System-scope writes use the trusted privilege runner, which removes loader-related environment variables. A missing optional provider is skipped. An attempted optional command that fails remains visible in diagnostics; after native success it is a warning and does not change exit status 0. A native update failure is fatal and prevents later optional updates.

Report command injection, wrong privilege or scope routing, executable-resolution bypasses, unsafe cache or mirror writes, and hidden transaction failures as security defects.
