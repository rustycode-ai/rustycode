# Orchestra Prompt Strategy

**Status:** Active prompt strategy
**Last Updated:** 2026-03-19

## Purpose

Prompts in Orchestra should be **narrow, contextual, and runtime-supported**.

Rust should do the heavy lifting for:

- state derivation
- context selection
- retry context injection
- evidence writing
- timeout and budget control

The LLM prompt should focus on the work itself.

## Prompt Design Rules

### 1. Prompt only the active unit

The model should receive one clear unit of work:

- plan this slice
- execute this task
- complete this slice
- validate this milestone

### 2. Include only relevant context

Typical context inputs:

- milestone and slice identifiers
- slice excerpt or roadmap excerpt
- task plan contract
- prior summaries in the same slice
- focused dependency/context snippets
- retry failure context when applicable

### 3. Keep control logic out of prompts

The runtime should not rely on prompts alone for:

- retry counts
- timeout enforcement
- budget policy
- state transitions
- evidence persistence

### 4. Use prompts for reasoning, not bookkeeping

Prompt for:

- implementation
- repair
- explanation
- planning

Avoid using prompt space for logic that Rust can enforce directly.

## Prompt Families

### Plan-slice prompt

Goal:

- turn slice context into a task plan
- create or update `PLAN.md`
- define verification expectations

### Execute-task prompt

Goal:

- implement the task plan in fresh context
- use tools to inspect, edit, and verify
- write required task artifacts

### Retry repair prompt

Goal:

- give the model capped, structured failure context from verification
- ask it to repair the specific failed checks
- avoid re-research and avoid broad context growth

### Complete-slice prompt

Goal:

- summarize completed tasks
- write slice summary artifacts
- update roadmap-related completion state

### Validate-milestone prompt

Goal:

- validate milestone completion once slices are done
- produce milestone-level validation artifacts

## Context Assembly Responsibilities

The runtime should assemble:

- task plan content
- slice context excerpt
- prior task summaries
- verification failure context
- skill context when applicable

The prompt should consume those assembled inputs, not rediscover them.

## Long-Term Prompt Goal

The best Orchestra prompts are short enough that their quality does not depend on remembering the whole project conversation.

That is the main reason to keep moving runtime responsibilities into Rust.
