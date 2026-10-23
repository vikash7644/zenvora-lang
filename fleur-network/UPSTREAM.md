# Agave Upstream Baseline

## Candidate upstream

- Repository: https://github.com/anza-xyz/agave
- Documentation: https://docs.anza.xyz/

Agave is the candidate base because Fleur's first plan is to reuse an existing validator/client implementation rather than write consensus from scratch. This choice is a proposal until a specific revision and its license/build requirements are reviewed.

## Baseline record

Fill these fields only after checking the actual upstream repository and running the commands:

- Selected tag/commit: **not selected**
- Review date: **pending**
- Build environment: **pending**
- Build command and result: **not run**
- Test command and result: **not run**
- Local-validator command and result: **not run**
- Known platform/resource limits: **pending**
- Fleur-specific patches: **none**

## Rules for a safe fork

1. Pin an exact upstream commit before implementation.
2. Keep upstream build and tests passing before Fleur-specific changes.
3. Separate Fleur branding/configuration from consensus or transaction semantics.
4. Do not modify cryptographic or consensus behavior without tests and review.
5. Track upstream security advisories and maintain a documented update path.
6. Never commit private keys, seed phrases, RPC credentials, or production secrets.

## First hands-on verification

Use a supported Linux environment with the dependencies specified by the pinned upstream revision. Follow that revision's own build and local-validator instructions rather than copying commands from an unrelated release. Record exact commands, exit status, and relevant logs here.