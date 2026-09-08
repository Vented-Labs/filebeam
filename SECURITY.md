# Security Policy

## Reporting

Report suspected vulnerabilities privately through the [GitHub vulnerability reporting form](https://github.com/Vented-Labs/filebeam/security/advisories/new) for `Vented-Labs/filebeam`. Do not open public issues for vulnerabilities. Include affected versions, reproduction steps, and impact.

## Trust Boundary

Filebeam encrypts and decrypts content in a browser worker. The service stores ciphertext and operational metadata, including transfer identifiers, type, expiry, ciphertext sizes, chunk counts, status, and capability-token hashes. Plaintext file and note contents, names, titles, and inner-manifest metadata stay in the browser.

This boundary does not protect against a compromised browser, frontend delivery path, dependency supply chain, endpoint, or browser extension. URL fragments are not sent in HTTP requests but can be disclosed through copying, syncing, screenshots, or compromised clients. Client-side encryption also does not hide existence, timing, sizes, or network metadata. No independent security audit has been completed.

See the [encryption protocol](encryption/README.md) for the format and key-custody details. Filebeam cannot recover a lost self-held recipient key or a private key wrapped with a forgotten password. A password reset restores account access, not prior encrypted key bundles; key replacement applies to future deliveries only.

## Burn On Read

Burn-on-read is available for notes. After successful decryption, a client submits its encrypted-manifest read token and the service queues deletion. It is best-effort removal, not exactly-once delivery: concurrent readers may already have a copy, and removal confirmation can fail after a successful read.

## Deployment

Use HTTPS, unique production secrets, protected database/storage/Redis credentials, patching, monitoring, access controls, and backups. Transfer limits are not a complete abuse-control system. Do not expose development Compose database or Redis ports or reuse development credentials in production.
