# Project dev env checklist

- [x] Chose package manager (uv, bun, go (cli), cargo)
- [x] linters (for all main used languages)
- [x] formatters (for all main used languages)
- [ ] tests
    - [x] unit tests (`#[cfg(test)]` stubs in each crate; wired via `cargo test`)
    - [ ] BDD e2e tests for main use cases
- [ ] Establish branch strategy (trunk-based vs GitFlow-lite) and naming conventions
- [ ] Define dependency update policy (cadence, approval rules, automation).
- [x] Define standard directory layout (src/, tests/, docs/, scripts/, infra/, etc.)
- [x] Provide “one command” workflows: run, test, lint, format, migrate, seed . Do it with `Makefile`.
- [x] Add a README with: local setup, runbooks links, test commands, release process
- [x] Add a AGENTS.md with main info for agents.
- [x] Pin runtime versions (e.g., toolchain versions), document upgrade path
- [x] Add pre-commit hooks (format, lint, secret scan, quick tests)
- [x] Ensure deterministic builds (lockfiles, vendoring policy if relevant)
- [ ] Establish local config handling: env template, safe defaults, validation
- [x] Add code coverage calculation and assert to >80%

In future (next month):
- [ ] Code health
    - [ ] Cognitive complexity
    - [ ] Halstead
    - [ ] Maintainability index
    - [ ] Code health index
- [ ] Class & modules metrics
    - https://speakerdeck.com/sobolevn/proiektirovaniie-eto-koghda-chuvstvuiesh-a-nie-kakiie-to-tam-tsifierki-nikolai-khitrov-pythonn?slide=77
    - https://youtu.be/eVcx6qZfU-M?si=CUKAQbkcDtsTPR9R
- [ ] Documentation metrics
