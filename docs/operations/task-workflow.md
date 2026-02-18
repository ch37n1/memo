---
description: Standard workflows for tasks of different sizes — from small fixes to large multi-phase efforts
---

# Task Workflow

## Overview

Every task follows the same core loop: **understand → implement → verify → document**. The depth of each phase scales with task size.

| Size | Context | Planning | Implementation | Key difference |
|------|---------|----------|----------------|----------------|
| Small (known context) | Provided, sufficient | None | Direct | Fastest path, no exploration needed |
| Small (unknown context) | Needs exploration | None | Direct | Self-guided context gathering before work |
| Mid | Always explore more | Plan with steps | Step by step | Structured plan, review gate before finish |
| Large | Always explore more | Plan with phases | Phase by phase | Decomposes into multiple mid-task processes |

---

## Common Steps

These steps appear across all workflows. Defined once, referenced by name.

### Context

| Step | Action |
|------|--------|
| **Read provided context** | Read all referenced materials: docs, tickets, code, conversations. |
| **Explore additional context** | Investigate codebase, related modules, dependencies. Form your own understanding. |
| **Clarify** | If context is still insufficient — ask questions. Do not guess on ambiguous requirements. |

### Verify

| Step | Action |
|------|--------|
| **Run automations** | Linters, formatters, tests. All must pass. |
| **Fix issues** | Resolve any failures from automations. Re-run until clean. |

### Document

| Step | Action |
|------|--------|
| **Check if docs need update** | Do changes affect documented behavior, architecture, or contracts? |
| **Update docs** | If yes — update relevant documentation. |
| **Update QA cases** | Add or update manual test cases for the most important scenarios. Keep it minimal — only critical paths and edge cases. |

---

## Workflows

### Small Task — Known Context

The context provided is sufficient to start immediately.

```
Read provided context
  │
  ├─ Not enough → Clarify
  │
  └─ Enough → Implement
                 │
                 Verify (automations → fix)
                 │
                 Document (docs → QA cases)
                 │
                 Done
```

1. Read provided context
2. If not enough — clarify, otherwise proceed
3. Implement
4. Verify
5. Document
6. Done

### Small Task — Unknown Context

Context needs self-guided exploration before implementation.

```
Read provided context
  │
  Explore additional context
  │
  ├─ Not enough → Clarify
  │
  └─ Enough → Implement
                 │
                 Verify (automations → fix)
                 │
                 Document (docs → QA cases)
                 │
                 Done
```

1. Read provided context
2. Explore additional context
3. If not enough — clarify, otherwise proceed
4. Implement
5. Verify
6. Document
7. Done

### Mid Task

Always requires deeper context exploration. Implementation follows a structured plan.

```
Read provided context
  │
  Explore additional context
  │
  ├─ Not enough → Clarify
  │
  └─ Enough → Create plan (steps)
                 │
                 Implement step by step
                 │
                 Verify (automations → fix)
                 │
                 Review changes
                 │
                 ├─ Needs fixes → back to Implement
                 │
                 └─ OK → Document (docs → QA cases)
                           │
                           Done
```

1. Read provided context
2. Explore additional context
3. If not enough — clarify, otherwise proceed
4. Create a plan with implementation steps
5. Implement step by step (single phase)
6. Verify
7. Review changes — if fixes needed, return to step 5
8. Document
9. Done

### Large Task

Decomposes into phases. Each phase is essentially a mid-task process.

```
Read provided context
  │
  Explore additional context
  │
  ├─ Not enough → Clarify
  │
  └─ Enough → Create plan (phases → steps)
                 │
                 For each phase:
                 │
                 ┌─────────────────────────┐
                 │  Mid-task workflow:      │
                 │  Plan steps → Implement  │
                 │  → Verify → Review       │
                 │  → Document              │
                 └─────────────────────────┘
                 │
                 All phases complete → Done
```

1. Read provided context
2. Explore additional context
3. If not enough — clarify, otherwise proceed
4. Create a plan with phases, each containing steps
5. Execute each phase as a mid-task process (plan steps → implement → verify → review → document)
6. Done
