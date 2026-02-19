# Documentation Agreements

## Purpose

Documentation serves as a comprehensive reference for the system. It is designed to help developers and AI agents understand, maintain, and extend the project. Every document must answer a real question — if it doesn't, delete it.

---

## Principles

| Principle | Description |
|-----------|-------------|
| **Docs are layered, not flat** | Each layer captures a different kind of truth (see [Documentation Layers](#documentation-layers)). |
| **Concept over implementation** | Describe *what* and *why*, not *how* at the file level. Implementation details change; concepts persist. Prefer _why_ over _what_. |
| **Business logic focus** | Capture all business logic and system behavior that influences the product. Document hacks and workarounds that affect behavior. |
| **Minimal code** | Include code snippets only when they significantly clarify non-trivial logic. Avoid duplicating the codebase in text form. |
| **Taxonomy laws** | Scale matters — boundaries must be strictly observed. Sort by relevance distance. Meaningful names matter. |
| **Docs should age well** | Separate conceptual docs from procedural docs. Keep architecture language-agnostic. Decision records are immutable once written. |
| **Module/Domain + Concept hybrid** | Organize by bounded contexts (navigation), explain through concepts (understanding). |
| **Mermaid diagrams** | Use text-based Mermaid diagrams for visual representations. |

### Documentation Layers

| Layer | Truth type | Example |
|-------|-----------|---------|
| Code comments | Local truth | Inline explanation of non-obvious logic |
| Design docs | Domain truth | Module responsibilities, flows, decisions |
| Architecture docs | System truth | Component boundaries, data flow, deployment |
| Decision records | Historical truth | Why a choice was made at a point in time |
| Exploration docs | Discovery truth | Users, needs, competitors research |
| Product vision docs | Vision truth | Goals, direction, strategy |

### Temporary Documents

Some documents are inherently time-bound and serve a specific phase:

| Document | Truth type |
|----------|-----------|
| PRD | Requirements truth |
| Roadmap | Planning truth |
| Implementation plan | Step-by-step execution truth |

---

## What to Include / Exclude

### Include

- System architecture and component responsibilities
- Processing flows and decision logic
- Business rules and constraints
- Edge cases and special handling
- Integration contracts and data formats
- Configuration and feature flags
- Hacks and workarounds with rationale

### Exclude

- Line-by-line code explanations
- Framework-specific boilerplate details
- Information already obvious from code or tool documentation
- Actual LLM prompt content (document rules and purpose instead, reference paths to prompts)
- Redundant information

---

## Best Practices

- Write docs as if **future-you is hostile**
- If a doc doesn't answer a real question, delete it
- Design docs mirror bounded context boundaries
- Start with a single file, split into a directory when it grows too large
- Keep architecture language-agnostic
- Decision records are worth their weight in gold

---

## Document Splitting Rules

When a document grows, use these rules to decide how to organize it. Split by **subdomain / bounded context**, not by individual concept.

### Terminology

| Term | Definition | Example |
|------|------------|---------|
| **Domain** | Top-level bounded context | `docs/design/payments/` |
| **Subdomain** | Significant functional area within a domain | `docs/design/payments/checkout/` |
| **Concept** | Specific behavior or mechanism (too small to split separately) | "Retry policy", "Token refresh" |

### When to Split

Split a document into a subdirectory when **any** of:

| Criterion | Threshold |
|-----------|-----------|
| Document length | > 300–500 lines |
| Subdomains | 2+ distinct functional areas |
| Source file count | 10+ files in the corresponding code |
| Integration points | Multiple external systems |

### Recursive Splitting

If a subdomain itself meets splitting criteria, split it further. Stop when:
- The document would be < 300 lines
- The unit is a single, cohesive behavior
- Further splitting breaks conceptual integrity

### Concept Merging

Don't create separate docs for small concepts. Merge related concepts into H2 sections within a single document.

### Decision Tree

```
Is the topic > 300 lines OR has 2+ distinct subdomains?
│
├─ NO → Single .md file (concepts as H2 sections)
│       Example: docs/design/auth.md
│
└─ YES → Create subdirectory with README.md
          │
          Example: docs/design/auth/
          ├─ README.md (overview + navigation)
          │
          └─ For each subdomain:
              │
              Is subdomain > 300 lines OR highly complex?
              │
              ├─ NO → subdomain.md (single file)
              │       Example: docs/design/auth/oauth.md
              │
              └─ YES → subdomain/ (nested directory)
                       Example: docs/design/auth/oauth/
                       ├─ README.md
                       ├─ concept-1.md
                       └─ concept-2.md
```

---

## Document Properties (Obsidian Frontmatter)

All documents use YAML frontmatter (Obsidian properties) for metadata.

### General Documents

Only `description` is required. Other properties are optional and added as needed.

```yaml
---
description: Brief summary of what this document covers
tags:
  - architecture
  - payments
aliases:
  - Payment Processing
---
```

| Property | Required | Description |
|----------|----------|-------------|
| `description` | yes | Brief summary of the document's content |
| `tags` | no | Obsidian tags for search and filtering |
| `aliases` | no | Alternative names for Obsidian linking |

### Decision Records

All properties are required (except where noted "if applicable").

```yaml
---
description: Why we chose PostgreSQL over MongoDB for the primary datastore
status: accepted
date: 2025-03-10
owners:
  - "@team-x"
supersedes: "0007"
superseded_by:
scope: payments
tags:
  - decision-record
  - database
---
```

| Property | Required | Values / format |
|----------|----------|-----------------|
| `description` | yes | Brief summary of the decision |
| `status` | yes | `accepted` \| `superseded` \| `deprecated` \| `rejected` |
| `date` | yes | `YYYY-MM-DD` |
| `owners` | yes | List of responsible people or teams |
| `supersedes` | if applicable | DR number this replaces |
| `superseded_by` | if applicable | DR number that replaces this |
| `scope` | yes | Domain or component the decision applies to |
| `tags` | yes | Must include `decision-record` |

---

## File Naming Conventions

| File | Purpose | When to use |
|------|---------|-------------|
| **README.md** | Entry point: overview + navigation for a directory | Always present in every docs subdirectory |
| **[concept].md** | Specific concept or subdomain documentation | For each documented topic |

Use `kebab-case` for file and directory names.

---

## Structure Patterns

**Simple topic** (< 300 lines):
```
docs/design/auth.md
```

**Split topic** (2+ subdomains):
```
docs/design/auth/
├── README.md          # Overview + navigation
├── oauth.md
└── sessions.md
```

**Deeply nested topic** (large subdomains):
```
docs/design/payments/
├── README.md          # Domain overview + subdomain links
├── checkout/
│   ├── README.md      # Subdomain overview
│   ├── flow.md
│   └── validation.md
└── refunds.md
```

---

## Decision Records

Decision records capture the reasoning behind significant choices at the time they were made. Metadata is stored in frontmatter (see [Document Properties](#document-properties-obsidian-frontmatter)).

### Rules

- Never rewrite old decision records — they are immutable snapshots
- Keep each record small and focused on a single decision
- Use `dr-` prefix for file names: `dr-0001-use-postgresql.md`
- Maintain a single index file (`README.md`) that lists all records with their status
- Add a note at the top: _"Decision records capture decisions at the time they were made. For current behavior, see `docs/architecture` and the code."_

---

## Archive

The `archive/` directory holds documents that are no longer actively maintained — superseded designs, outdated plans, or deprecated agreements. Archived docs are kept for historical reference only.

### Rules

- Move a document to `archive/` instead of deleting it when it still has historical value
- Do not update archived documents; they are read-only snapshots
- Add a note at the top of each archived file: _"This document is archived and no longer current. See [replacement link] for current information."_
- Flat structure preferred — no subdirectories unless volume demands it

---

## Docs Directory Structure

| Directory | Purpose | Truth type |
|-----------|---------|------------|
| `docs/README.md` | Entry point: project overview, docs map, links | — |
| `overview` | Problem space, goals, non-goals, glossary | Vision truth |
| `architecture` | System context, high-level design, data flow, deployment | System truth |
| `design/` | Per bounded context: responsibilities, flows, decisions (closer to implementation, less abstract) | Domain truth |
| `decision-records/` | Numbered records of significant choices with status tracking | Historical truth |
| `development/` | Local setup, coding guidelines, testing strategy | Procedural truth |
| `operations/` | Configuration, monitoring, documentation agreements (this file) | Operational truth |
| `references/` | API specs, data model, external links | Lookup material |
| `archive/` | Deprecated or superseded documents no longer actively maintained | Historical truth |


Example:

```txt
docs/
├── README.md                  # Entry point: project overview, docs map, link to this file
│
├── overview.md                  # Problem space and goals (for now not needed, README is ok)
│
├── architecture.md              # System-level truth (in this project architecture.md is enough)
│
├── design/                    # Domain-level truth (per bounded context), closer to implementation, less abstract
│   ├── README.md
│   ├── <domain-a>.md
│   └── <domain-b>/
│       ├── README.md
│       └── ...
│
├── decision-records/          # Historical truth
│   ├── README.md              # Index of all decisions
│   ├── dr-0001-example.md
│   └── ...
│
├── development/               # How to work with the project
│   ├── README.md
│   ├── local-setup.md
│   ├── coding-guidelines.md
│   └── testing-strategy.md
│
├── operations/                # How to run and maintain the project
│   ├── README.md
│   ├── documentation.md       # ← this file
│   ├── configuration.md
│   └── monitoring.md
│
├── references/                # Lookup material
│   ├── api.md
│   └── data-model.md
│
└── archive/                   # Deprecated or superseded documents
    └── ...
```


