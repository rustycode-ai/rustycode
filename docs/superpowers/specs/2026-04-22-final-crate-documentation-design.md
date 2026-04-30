# Final Crate Documentation: rustycode-tools, rustycode-tui, rustycode-web

**Design Document**  
Date: 2026-04-22

---

## Overview

Complete RustyCode's crate documentation by writing comprehensive READMEs for the three largest, most complex crates: rustycode-tools (66K LOC), rustycode-tui (92K LOC), and rustycode-web (1.2K LOC). These are the final undocumented crates; 47 others have already been documented following an established template.

## Goal

Document all three crates with **comprehensive, dual-mode analysis**: describe current architecture while mapping toward the intended future modular state. Identify refactoring opportunities and explain god object status honestly.

## Approach: Integrated Narrative with Refactoring Notes

Each README will:
1. Describe current architecture with inline refactoring opportunities
2. Include honest "Known Limitations & God Object Status" section
3. Propose intended future architecture with rationale
4. Document public API, key types, and usage patterns
5. Explain dependencies and design rationale

**Structure Template (all three crates):**

```
# [Crate Name]

[One-sentence description]

## Purpose
What this crate does, why it exists, what systems depend on it

## Current Architecture
Describes existing module organization, key types, patterns
[Inline notes: "The [module] (currently X files) is a candidate for splitting 
into [proposed modules] because..."]

## Key Types & Public API
Major types, trait definitions, main entry points
Usage examples showing how consumers interact

## Features
Major capabilities and subsystems

## Known Limitations & God Object Status
Honest assessment: what's too large, what needs refactoring, why

## Intended Future Architecture
Describes proposed modular breakdown
Explains timeline/priority of refactoring

## Dependencies
External crates and cross-crate dependencies

## Architecture Notes
Design patterns, decision rationale, how to extend

## Testing
How to test this crate, coverage status

## See Also
Related crates and documentation
```

## Crate-Specific Characteristics

### rustycode-tools (104 files, 66K LOC)

**Current state:** Monolithic tool execution framework
- executor/ — Tool execution pipeline
- security/ — Permission system, sandbox validation
- providers/ — Tool provider implementations
- Other modules — Utilities, formatters, helpers

**Proposed future:** Split into focused modules
- executor/ → stays as-is
- security/ → separate crate or clear module boundary
- registry/ → Extract tool metadata/discovery
- providers/ → Separate from core executor

**Documentation focus:**
- Executor pipeline and lifecycle
- Security model and permission system
- Provider abstraction and implementations
- Integration points with rustycode-llm and rustycode-orchestration

### rustycode-tui (244 files, 92K LOC)

**Current state:** Large UI crate with services, memory, workspace
- services/ — Agent mode, checkpoint, MCP mode, conversation, session, etc.
- memory/ — Memory management, compaction, injection
- workspace/ — Workspace context, progress, scanning
- app/ — UI components and handlers
- theme, unicode, logging — Infrastructure

**Proposed future:** Thin UI facade with extracted service layers
- services/ → Extract to separate service crates or clear boundaries
- memory/ → Leverage rustycode-vector-memory, rustycode-learning
- workspace/ → Boundary-clarify with rustycode-session
- app/ → Minimal UI renderer

**Documentation focus:**
- Service architecture and lifecycle
- State management patterns
- Memory and context injection
- Event handling and UI updates
- Integration with agents and providers

### rustycode-web (4 files, 1.2K LOC)

**Current state:** Small web integration layer
- Straightforward, stable

**Proposed future:** Minor refactoring if needed, mostly stable

**Documentation focus:**
- Purpose and use cases
- API contract
- Integration points

## Depth Level

**Comprehensive:** Deep analysis including:
- Module organization and rationale
- Key design patterns and why they were chosen
- Known issues and technical debt
- Refactoring strategy and timeline
- Integration with other crates

## Success Criteria

- ✅ All three READMEs completed
- ✅ Current architecture clearly documented
- ✅ Proposed future state described with rationale
- ✅ Public API documented with examples
- ✅ Refactoring opportunities clearly noted
- ✅ Each README stands alone but cross-references others
- ✅ Content accuracy verified
- ✅ Consistent with established documentation pattern (47 existing crates)
- ✅ All committed together with clear messaging

## Implementation Strategy

**Execution method:** Parallel subagent-driven approach
- Dispatch three subagents simultaneously (one per crate)
- Each analyzes crate, writes comprehensive README
- Reviews and iterations in parallel
- All three committed together

**Timeline:** Single batch completion

## Dependencies & Integration

- **Pattern source:** Existing 47 crate READMEs (protocol, core, llm, tools-api, tools, bus, storage, etc.)
- **Context:** Architecture review (ARCHITECTURE-REVIEW-2026-04-20.md) identifying god objects
- **Output location:** `crates/rustycode-tools/README.md`, `crates/rustycode-tui/README.md`, `crates/rustycode-web/README.md`

## Architecture Decisions

1. **Dual-mode documentation** — Current state + intended future, integrated narrative
   - Why: Helps maintainers understand evolution path and refactoring rationale
   - Trade-off: Requires careful writing to clarify what exists vs. what's planned

2. **Parallel execution** — Write all three simultaneously
   - Why: Efficient, all done in one phase
   - Trade-off: Requires coordinating three subagents

3. **Honest god object assessment** — Include "Known Limitations" section
   - Why: Transparency about technical debt helps future decisions
   - Trade-off: Some might see it as negative, but it's accurate

4. **Cross-crate references** — Each README mentions the others
   - Why: Shows interdependencies and integration pattern
   - Trade-off: Requires keeping references consistent

## Risk Mitigation

- **Risk:** Documentation becomes outdated as code evolves
  - Mitigation: Include maintenance notes in each README

- **Risk:** Proposed future state is too aspirational
  - Mitigation: Base it on existing architecture review and refactoring plans

- **Risk:** Inconsistency with 47 existing READMEs
  - Mitigation: Use same template and review against pattern

## Next Steps (After Approval)

1. ✅ This spec approved by user
2. → Create implementation plan (writing-plans skill)
3. → Execute via subagent-driven-development
4. → Commit all three READMEs
5. → Verify final documentation completeness

---

**Status: Ready for implementation planning**
