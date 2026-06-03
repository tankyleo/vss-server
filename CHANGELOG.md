# 0.1.0-alpha.0 - Jun. 11, 2026

- VSS service implementing VSS protocol version 0. (#34, #35)
- Rust workspace for the VSS server, API contract types, authentication implementations, and storage backend
  implementations. (#34, #35, #43, #72, #79, #101)
- PostgreSQL storage backend with database initialization, migrations, TLS support, key-level versioning, store-level
  global versioning, transactional writes, deletes, and paginated key-version listing. (#35, #55, #67, #96)
- Signature-based and JWT-based authorization implementations, with cfg-gated no-op authorization for local development
  and tests. (#34, #43, #72, #79, #87)
- Configuration through TOML file and environment variables, including bind address, request body size, logging, JWT
  RSA public key, and PostgreSQL settings. (#46, #67, #72, #73, #76, #87)
- Server logging to stdout/stderr and file, with SIGHUP log-file reopening and shutdown on CTRL-C/SIGTERM. (#34, #87)
- Prometheus-compatible `/metrics` health metric. (#99)
- Docker and Docker Compose files for local deployment. (#76, #80)
- Getting-started documentation. (#102)

In total, this release features 42 files changed, 7956 insertions from 14 authors in alphabetical order:

- Andrei
- Arik
- benthecarman
- dzdidi
- Elias Rohrer
- Enigbe
- fmar
- G8XSU
- Gursharan Singh
- Jeffrey Czyz
- Leo Nash
- Matt Corallo
- Steve Lee
- tankyleo
