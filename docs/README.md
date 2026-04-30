# RustyCode Documentation

This is the documentation hub for the repository. If you are not sure where to start, start here.

## Choose By Goal

### Use RustyCode

1. [Quickstart](guides/QUICKSTART.md)
2. [Tutorial](guides/TUTORIAL.md)
3. [Troubleshooting](guides/troubleshooting.md)
4. [Integration Guide](guides/INTEGRATION.md)

### Contribute Or Develop

1. [Developer Guide](guides/developer-guide.md)
2. [Contributing](../CONTRIBUTING.md)
3. [Agent Governance](project/agent-governance.md)
4. [Reference Index](reference/index.md)

### Understand The System

1. [System Architecture](architecture/architecture.md)
2. [Architecture Review](architecture/ARCHITECTURE-REVIEW-2026-04-20.md)
3. [Orchestra Architecture](orchestra/orchestra-architecture.md)
4. [Architecture Decisions](adr/)

## Documentation Map

| Area | Use It For |
| --- | --- |
| [guides/](guides/) | Reader-oriented docs for setup, workflows, troubleshooting, and developer onboarding |
| [architecture/](architecture/) | Current system structure, subsystem relationships, and integration notes |
| [orchestra/](orchestra/) | Canonical docs for autonomous mode behavior, workflow, commands, and prompts |
| [reference/](reference/) | Stable reference material such as APIs, specs, standards, permissions, and release notes |
| [adr/](adr/) | Architecture decisions that record why key choices were made |
| [security/](security/) | Provider security assumptions and review checklists |
| [project/](project/) | Project operating rules and contributor-facing governance |
| [design/](design/) | Working design documents and proposal-stage material |
| [research/](research/) | Exploratory investigations and comparative analysis |
| [superpowers/](superpowers/) | Active planning and implementation artifacts for ongoing work |
| [diagrams/](diagrams/) | Supporting diagrams and visual aids |
| [crates/](crates/) | Workspace-level crate documentation index |

Most top-level sections now have a local `README.md` or index page. If you add a new section, give it a landing page immediately.

## Stable Vs Working Docs

Use these sections differently:

- Stable docs: `guides`, `architecture`, `orchestra`, `reference`, `adr`, `security`, `project`
- Working docs: `design`, `research`, `superpowers`

If you are adding a new long-lived document, prefer a stable section. If you are capturing analysis, planning, or in-flight design work, place it in a working section.

## Root-Level Docs

Only a small set of documents should live at the repository root:

- [README.md](../README.md) for repo entry
- [CONTRIBUTING.md](../CONTRIBUTING.md) for contributor workflow
- [CLAUDE.md](../CLAUDE.md) for project-wide development instructions
- [TEAM_LEARNINGS.md](../TEAM_LEARNINGS.md) because the runtime reads and updates it directly

## Docs Outside `docs/`

Some documentation stays close to the subsystem it describes:

- [scripts/README.md](../scripts/README.md) for automation scripts
- [tests/README.md](../tests/README.md) and related test reports under `tests/`
- crate-specific READMEs under `crates/*/README.md`

## If You Are Cleaning Up Docs

1. Put the canonical version in one place.
2. Link it from this hub or the relevant section index.
3. Leave short compatibility stubs only when an old location is still referenced by tools or people.
