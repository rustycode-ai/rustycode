use crate::codec::HookInput;
use crate::codec::HookResult;

// Evaluate permission requests.
pub fn evaluate(_input: &HookInput) -> HookResult {
    // Simple policy: allow by default (hook-based enforcement is handled in pre_tool).
    HookResult::allow()
}
