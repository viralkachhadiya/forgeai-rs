# forgeai-rs Roadmap

## Milestone 1 (0.1.0 foundation)

- [x] Cargo workspace scaffold
- [x] Core domain types and adapter traits
- [x] High-level `forgeai::Client`
- [x] Provider adapter crate stubs
- [x] CI + release workflow scaffolding
- [ ] Publish `forgeai-core` and `forgeai`

## Milestone 2 (adapter functionality)

- [ ] Implement real OpenAI chat and streaming adapter
- [ ] Add integration tests with mocked HTTP and live opt-in tests
- [ ] Implement Anthropic and Gemini functional parity
- [ ] Generate capability matrix from tests

## Milestone 3 (advanced features)

- [ ] Tool execution runtime
- [ ] Structured output schema derive support
- [ ] Retry/fallback/router policies
- [ ] OpenTelemetry + Prometheus integration

## Milestone 4 (production hardening)

- [ ] Record/replay harness
- [ ] Gateway service endpoints and auth
- [ ] Security and dependency audit pipeline
- [ ] Semver and deprecation policy docs

## Publish order

1. `forgeai-core`
2. `forgeai-stream`, `forgeai-tools`, `forgeai-schema`
3. Adapter crates
4. `forgeai-router`, `forgeai-observability`, `forgeai-replay`
5. `forgeai`
6. `forgeai-gateway`
