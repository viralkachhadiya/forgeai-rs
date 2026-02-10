# ADR-0001: Clean-Room Migration Constraints

Date: 2026-02-10

## Decision

Implement `forgeai-rs` from public requirements and provider docs without copying implementation details from reference code.

## Consequences

- Independent naming and module boundaries.
- Contract tests for behavior parity.
- Lower legal risk for open-source distribution.
