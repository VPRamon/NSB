use std::fs;
use std::path::PathBuf;

#[test]
fn promotion_workflow_updates_existing_branch_with_lease() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.github/workflows/starlight-final-promotion.yml");
    let workflow = fs::read_to_string(&path).expect("read final promotion workflow");
    assert!(
        workflow.contains("--force-with-lease="),
        "repeated promotion PR updates must use force-with-lease"
    );
    assert!(
        !workflow.contains("git push --force ") && !workflow.contains("git push -f "),
        "unrestricted force push is forbidden"
    );
    assert!(workflow.contains("fail-closed"));
    assert!(workflow.contains("promotion assets unchanged"));
    assert!(workflow.contains("starlight/production-promotion"));
}
