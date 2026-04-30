use crate::codec::HookInput;

pub fn evaluate(_input: &HookInput) -> crate::codec::HookResult {
    // PostToolUse: allow by default. Advisory warnings can be added per-rule
    // when specific post-condition checks are needed (e.g., unexpected output patterns).
    crate::codec::HookResult::allow()
}
