# Active Reasoning Engine — Design Sketch

**Problem:** Historical thinking prototypes were reactive — they structured thoughts after the LLM had already started wandering. They suggested strategy changes only *after* detecting stagnation.

**Solution:** An **active workflow engine** that sequences the LLM through structured problem-solving phases before implementation begins.

## Core Insight

The difference:
- **Passive (historical deep-thinking prototype):** LLM thinks → system detects stuck state → suggests strategy switch
- **Active (desired):** System says "phase 1: decompose" → LLM breaks problem into modules → phase 2 begins

## The Workflow (4 Phases)

### Phase 1: Problem Decomposition
**Tool:** `decompose_problem`

**Input:**
- Goal/task statement
- Context (what problem domain?)

**Output:**
- 3-5 critical submodules/subproblems
- Dependencies between modules
- Open questions per module
- Confidence in decomposition (0-1)

**Example:**
```
Goal: "Build a Rust CLI tool that accepts credentials securely"

Decomposition:
├─ Module A: Secret input handling
│  ├─ How to read passwords without echoing?
│  ├─ What Rust crates exist?
│  └─ Confidence: 0.6 (need research)
├─ Module B: Credential storage
│  ├─ In-memory? Persistent? Encrypted?
│  └─ Confidence: 0.4 (need requirements)
└─ Module C: Security validation
   ├─ What attacks to prevent?
   └─ Confidence: 0.3 (need threat model)
```

### Phase 2: Research Directive
**Tool:** `guide_research`

**Input:**
- Module from Phase 1
- Open question
- Known constraints

**Output:**
- Prioritized research targets (existing implementations, docs, RFCs, standards)
- Search queries to try
- Why each matters for this module
- Expected findings

**Example:**
```
Module: Secret input handling
Question: How to read passwords without echoing?

Research targets (priority):
1. Rust crate: rpassword, dialoguer, secret-keeper
   → Check source, read docs, understand API
2. Unix system calls: getpass(), tcgetattr()
   → Understand OS-level mechanism
3. Windows API: ReadConsoleInputW
   → Handle cross-platform differences

Expected findings:
- Which crate is most maintained?
- Does it handle Unicode?
- What about clipboard attacks?
```

**Action:** LLM goes research these, reports back findings.

### Phase 3: Goal Clarification
**Tool:** `validate_requirements`

**Input:**
- Module
- Findings from research
- Current assumptions

**Output:**
- Clarified requirements (before/after)
- Assumption validity (what's confirmed? what's still unclear?)
- Next questions to answer
- Readiness to implement (0-1)

**Example:**
```
Module: Secret input handling

Before: "Use a crate that hides password input"

After research findings:
- `rpassword` is most maintained (500+ stars, recent commits)
- It wraps Unix getpass() + Windows APIs
- Does NOT handle clipboard/shoulder-surfing
- Limitation: Can't set input buffer size

Clarified requirement:
"Use rpassword for echo hiding. For clipboard attacks, document
this as a known limitation. Consider adding terminal-clear-screen
as a companion measure."

Readiness: 0.8 (can implement)
```

### Phase 4: Integration Check
**Tool:** `check_integration`

**Input:**
- Module design (from phases 1-3)
- Adjacent modules' designs
- Full system context

**Output:**
- Integration risks identified
- Design conflicts
- Dependencies validated
- Go/no-go for implementation

**Example:**
```
Module: Secret input handling
Adjacent: Credential storage (needs to receive secret securely)

Integration check:
✓ Secret input → memory buffer
✓ Memory buffer → passed to storage module
✓ Storage clears buffer after use
✓ No accidental logging

✗ CONFLICT: What if credential storage crashes?
  → Secret left in memory?
  → Need recovery protocol

Go/no-go: CONDITIONAL
→ Implement with error handling for storage failures
```

---

## The Loop

**One complete cycle:**

```
User: "Build a secure credential handler"
    ↓
[Phase 1] decompose_problem
    → Outputs: 3 modules, questions, confidence
    ↓
User reads decomposition, confirms/adjusts
    ↓
[Phase 2] guide_research (for each module)
    → Guides LLM to research targets
    ↓
User/LLM researches, reports findings
    ↓
[Phase 3] validate_requirements
    → Confirms assumptions, clarifies goals
    ↓
[Phase 4] check_integration
    → Validates all pieces fit
    ↓
READY TO IMPLEMENT
    ↓
User: "Implement module A using findings from phase 3"
    ↓
[Continue with B, C...]
```

**If clarity drops (confidence < 0.5 after phase 3):**
- Loop back to phase 2 for more research
- Adjust phase 1 decomposition
- Repeat phase 3

---

## Key Differences from Deep-Thinker

| Aspect | Deep-Thinker | Active Engine |
|--------|--------------|---------------|
| Timing | Reacts after thinking | Guides before thinking |
| Trigger | Detects stagnation | Enforces workflow |
| Output | "Try dialectic" | "Research these 3 things" |
| User Control | LLM chooses what to do | LLM follows phases |
| Clarity | Implicit (via confidence) | Explicit (readiness scores) |

---

## Implementation Points for RustyCode

### 1. Tool Definitions (MCP)
```rust
// In rustycode-tools or new rustycode-reasoning crate

pub struct DecompositionRequest {
    pub goal: String,
    pub context: String,
}

pub struct Module {
    pub name: String,
    pub questions: Vec<String>,
    pub dependencies: Vec<String>,
    pub confidence: f32,
}

pub fn decompose_problem(req: DecompositionRequest) -> Result<Vec<Module>> {
    // Call LLM with structured prompt
    // Parse response into Module structs
}
```

### 2. Workflow Orchestration
New crate: `rustycode-reasoning` or extend `rustycode-orchestration`

```rust
pub enum ReasoningPhase {
    Decompose,
    Research,
    Clarify,
    Integrate,
}

pub struct ActiveReasoning {
    current_phase: ReasoningPhase,
    goal: String,
    modules: Vec<Module>,
    // ...
}

impl ActiveReasoning {
    pub async fn advance(&mut self) -> Result<PhaseOutput> {
        match self.current_phase {
            Decompose => { /* phase 1 */ }
            Research => { /* phase 2 */ }
            // ...
        }
    }
}
```

### 3. Integration with Autonomous Mode
```rust
// In autonomous development workflow:

orchestrator.start_reasoning(user_goal);
while !orchestrator.ready_to_implement() {
    phase_output = orchestrator.advance_phase().await?;
    // Display phase_output to user or auto-validate
}

// Now safe to generate/implement code
generated_code = orchestrator.generate_implementation().await?;
```

### 4. Prompting Strategy
Instead of generic "think deeply", use phase-specific prompts:

**Phase 1 (Decompose):**
```
You are a problem-decomposition specialist.
For this goal: {goal}

MUST return exactly this JSON format:
{
  "modules": [
    {
      "name": "...",
      "description": "...",
      "questions": ["?", "?"],
      "dependencies": ["module_x"],
      "confidence": 0.7
    }
  ]
}
```

**Phase 2 (Research):**
```
You are a research guide.
For module "{module_name}":
Question: {open_question}

MUST recommend:
1. Top 3 specific resources (crates, docs, RFCs, standards)
2. Why each one matters
3. What to look for in each
4. How findings will help

Format as structured list.
```

### 5. Readiness Validation
```rust
pub fn calculate_readiness(
    modules: &[Module],
    phase: ReasoningPhase,
) -> f32 {
    match phase {
        Clarify => {
            // Average confidence > 0.7?
            // All questions answered?
            // Dependencies mapped?
        }
        Integrate => {
            // No conflicts?
            // All modules have error handling?
            // Tested integration plan?
        }
    }
}
```

---

## Example Interaction

```
User: "I need to add API rate limiting to our service"

[DECOMPOSE]
System: "Breaking down rate-limiting system..."
→ Output:
  Module 1: Rate limit strategy (token bucket? sliding window?)
  Module 2: Storage backend (in-memory? Redis? DynamoDB?)
  Module 3: Enforcement hooks (where to inject checks?)
  Module 4: Observability (metrics, alerts)

User: Reads decomposition, confirms it's right direction

[RESEARCH Phase 1]
System: "Researching rate-limit algorithms..."
  - Check: token-bucket-algorithm standard
  - Check: existing Rust crates (governor, ratelimit-meter)
  - Check: comparison blog posts
  
User: "Found that governor crate is most maintained"

[RESEARCH Phase 2]
System: "Researching storage backends for rate-limit state..."
  - Check: Redis vs in-memory tradeoffs
  - Check: failure modes (if Redis is down, what happens?)
  
User: "For our scale, in-memory + async backup to Redis"

[CLARIFY]
System: "Validating against your constraints..."
→ "Your design is solid. Readiness: 0.85"
  Assumptions confirmed:
  ✓ governor crate viable
  ✓ In-memory primary, Redis backup is standard pattern
  ✓ You'll need graceful degradation if Redis unavailable

[INTEGRATE]
System: "Checking if this fits your architecture..."
  → No conflicts with existing auth system
  → Logging system can absorb metrics
  
READY TO IMPLEMENT

User: "Generate code for the rate limiter"
System: Uses all findings from phases 1-4 to generate
```

---

## Why This Works for Deep Thinking

1. **Proactive:** Guides thinking *before* implementation
2. **Structured:** Clear phases, not vague "think harder"
3. **Concrete:** "Research token-bucket algorithms" not "try parallel reasoning"
4. **Validating:** Readiness scores confirm you actually understand
5. **Iterative:** If a phase reveals gaps, loop back (not stuck detecting)
6. **Testable:** Each phase has clear input/output

---

## Next Steps

1. **Prototype Phase 1** (decompose_problem tool) in current stack
   - Use existing LLM provider interface
   - Parse structured JSON output
   - Validate against simple examples

2. **Add Phase 2** (guide_research)
   - Integrate with web search or documentation crawlers
   - Prioritize research targets

3. **Add Phase 3** (validate_requirements)
   - Cross-reference research findings against assumptions
   - Calculate readiness score

4. **Integrate into Autonomous Mode**
   - Before generating implementation, run active reasoning
   - Use phase outputs to inform code generation

---

## Related

- historical deep-thinker: structured thought graphs (good foundation)
- rustycode-orchestration: orchestration engine (where this lives)
- rustycode-llm: LLM provider interface (needed for tool calls)
- CLAUDE.md: workflow guidance (inspiration for this design)
