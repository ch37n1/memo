# Project dev env checklist

- [ ] Chose package manager (uv, bun, go (cli), cargo)
- [ ] linters (for all main used languages)
- [ ] formatters (for all main used languages)
- [ ] tests
    - [ ] unit tests
    - [ ] BDD e2e tests for main use cases 
- [ ] Establish branch strategy (trunk-based vs GitFlow-lite) and naming conventions
- [ ] Define dependency update policy (cadence, approval rules, automation).
- [ ] Define standard directory layout (src/, tests/, docs/, scripts/, infra/, etc.)
- [ ] Add a README with: local setup, runbooks links, test commands, release process
- [ ] Add a AGENT.md with main info for agents.
- [ ] Pin runtime versions (e.g., toolchain versions), document upgrade path
- [ ] Provide “one command” workflows: run, test, lint, format, migrate, seed
- [ ] Add pre-commit hooks (format, lint, secret scan, quick tests)
- [ ] Ensure deterministic builds (lockfiles, vendoring policy if relevant)
- [ ] Establish local config handling: env template, safe defaults, validation
- [ ] Processes description in readme.md and agent.md


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
