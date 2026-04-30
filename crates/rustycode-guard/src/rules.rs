#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GuardAction {
    Deny,
    Ask,
    Warn,
}

#[derive(Debug)]
pub struct GuardRule {
    pub id: &'static str,
    pub description: &'static str,
    pub action: GuardAction,
}

pub fn all_rules() -> Vec<&'static GuardRule> {
    vec![
        &R01_SUDO,
        &R02_PROTECTED_PATHS,
        &R03_SHELL_WRITES,
        &R04_OUTSIDE_WORKSPACE,
        &R05_RM_RF,
        &R06_FORCE_PUSH,
        &R07_SECRETS_IN_CONTENT,
        &R08_BINARY_WRITE,
        &R09_PATH_TRAVERSAL,
        &R10_NO_VERIFY,
        &R11_HARD_RESET_MAIN,
        &R12_PUSH_MAIN,
        &R13_CONFIG_EDITS,
        &R14_SYMLINK,
        &R15_RESOURCE_LIMITS,
    ]
}

pub const R01_SUDO: GuardRule = GuardRule {
    id: "R01",
    description: "Block sudo commands",
    action: GuardAction::Deny,
};

pub const R02_PROTECTED_PATHS: GuardRule = GuardRule {
    id: "R02",
    description: "Block writes to .git/, .env, secret files",
    action: GuardAction::Deny,
};

pub const R03_SHELL_WRITES: GuardRule = GuardRule {
    id: "R03",
    description: "Block shell writes to protected paths",
    action: GuardAction::Deny,
};

pub const R04_OUTSIDE_WORKSPACE: GuardRule = GuardRule {
    id: "R04",
    description: "Block edits outside workspace",
    action: GuardAction::Deny,
};

pub const R05_RM_RF: GuardRule = GuardRule {
    id: "R05",
    description: "Block 'rm -rf' commands",
    action: GuardAction::Deny,
};

pub const R06_FORCE_PUSH: GuardRule = GuardRule {
    id: "R06",
    description: "Block git push --force",
    action: GuardAction::Deny,
};

pub const R07_SECRETS_IN_CONTENT: GuardRule = GuardRule {
    id: "R07",
    description: "Block secrets in content (sk-, ghp-, AKIA)",
    action: GuardAction::Deny,
};

pub const R08_BINARY_WRITE: GuardRule = GuardRule {
    id: "R08",
    description: "Block writing binary extensions",
    action: GuardAction::Deny,
};

pub const R09_PATH_TRAVERSAL: GuardRule = GuardRule {
    id: "R09",
    description: "Block path traversal '..'",
    action: GuardAction::Deny,
};

pub const R10_NO_VERIFY: GuardRule = GuardRule {
    id: "R10",
    description: "Block '--no-verify' / '--no-gpg-sign' in bash",
    action: GuardAction::Deny,
};

pub const R11_HARD_RESET_MAIN: GuardRule = GuardRule {
    id: "R11",
    description: "Block 'git reset --hard main/master'",
    action: GuardAction::Deny,
};

pub const R12_PUSH_MAIN: GuardRule = GuardRule {
    id: "R12",
    description: "Block 'git push origin main/master'",
    action: GuardAction::Deny,
};

pub const R13_CONFIG_EDITS: GuardRule = GuardRule {
    id: "R13",
    description: "Block edits to config-style files",
    action: GuardAction::Deny,
};

pub const R14_SYMLINK: GuardRule = GuardRule {
    id: "R14",
    description: "Block symlink path usage",
    action: GuardAction::Deny,
};

pub const R15_RESOURCE_LIMITS: GuardRule = GuardRule {
    id: "R15",
    description: "Block overly large content payloads",
    action: GuardAction::Deny,
};
