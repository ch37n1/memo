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
    - [ ] For all tasks we must do these steps (unless said opposite):
        - check if there is skill avaliable for task
        - if not ready on local, then search with `Find Skills` skill
        - if not found then do not use skill and do by yourself
        - recheck all change with review agent
        - tests step (add & check)
        - documentation update step (check if doc need to be updated and do so if need)

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
