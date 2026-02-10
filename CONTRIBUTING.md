# Contributing

## Setup

1. Install stable Rust via `rustup`.
2. Run `cargo fmt --all`.
3. Run `cargo clippy --workspace --all-targets -- -D warnings`.
4. Run `cargo test --workspace`.

## Pull Requests

- Keep changes focused.
- Add tests for behavior changes.
- Update docs for public API changes.
- Follow semver and deprecate before breaking.
