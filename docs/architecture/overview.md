# Architecture Overview

`forgeai-rs` uses a trait-based adapter architecture.

- `forgeai-core` defines domain contracts.
- Adapter crates implement provider-specific behavior.
- `forgeai` provides a simple unified client.
