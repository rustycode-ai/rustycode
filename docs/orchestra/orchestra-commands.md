# Orchestra Commands

**Status:** Current + planned command model
**Last Updated:** 2026-03-19

## Purpose

This document separates:

- commands that exist today in some form
- commands that are planned or partial

It should not be read as a promise that every public Orchestra command is already fully implemented in RustyCode.

## Current Focus

The current implementation focus is the autonomous runtime path around `orchestra auto` and the `.orchestra/` execution model.

## Current or Partially Available Commands

These exist in the codebase in some form, though depth and parity vary:

- `orchestra init`
- `orchestra progress`
- `orchestra state`
- `orchestra auto`
- milestone/slice/task management commands in the legacy Orchestra CLI surface

## Planned / Incomplete Commands

These are useful command concepts but should be treated as roadmap items unless explicitly verified in the code path being used:

- guided discuss / planning flows
- quick mode parity with public Orchestra
- richer verification and ship flows
- more complete worktree lifecycle commands
- deeper preferences/config commands for Orchestra v2 runtime

## Command Principles

### Commands should map to runtime capabilities

We should not advertise commands as fully supported unless the runtime can actually:

- derive state correctly
- execute the unit reliably
- persist artifacts correctly
- verify outcomes deterministically

### Prefer truth over breadth

A smaller set of trustworthy commands is better than a broad but misleading command surface.

## Recommended Future Command Grouping

### Project and state

- `orchestra init`
- `orchestra progress`
- `orchestra state`

### Autonomous runtime

- `orchestra auto`
- `orchestra auto --budget ...`
- future: pause/resume/status controls backed by runtime state

### Planning and milestone management

- milestone creation / completion
- slice planning
- milestone validation

### Diagnostics

- health/doctor commands
- verification evidence inspection
- metrics and budget reporting

## Documentation Rule

If command docs need to show aspirational behavior, label it clearly as `planned` rather than presenting it as current fact.
