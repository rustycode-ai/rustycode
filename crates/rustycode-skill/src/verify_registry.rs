use crate::bundled::get_bundled_skills;
#[test]
fn verify_bundled_skills_registration() {
    let skills = get_bundled_skills();
    assert!(skills.iter().any(|s| s.id == "research"));
    assert!(skills.iter().any(|s| s.id == "debug"));
    assert!(skills.iter().any(|s| s.id == "worktree"));
}
