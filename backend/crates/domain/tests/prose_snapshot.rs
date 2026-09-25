//! `policy/refund-policy.md` is the rendered default policy, committed so readers
//! can see it without running anything. This test fails when the file is stale.
//! Regenerate it with `UPDATE_POLICY_MD=1 cargo test -p domain --test prose_snapshot`.

use domain::policy::Policy;
use domain::prose::render_policy;

const DEFAULT_POLICY: &str = include_str!("../../../../policy/default-policy.json");
const POLICY_MD_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../policy/refund-policy.md"
);

#[test]
fn committed_policy_markdown_matches_the_renderer() {
    let rendered = render_policy(&Policy::parse(DEFAULT_POLICY).unwrap());
    if std::env::var_os("UPDATE_POLICY_MD").is_some() {
        std::fs::write(POLICY_MD_PATH, &rendered).unwrap();
    }
    let committed = std::fs::read_to_string(POLICY_MD_PATH).unwrap();
    assert_eq!(
        committed, rendered,
        "policy/refund-policy.md is stale; rerun with UPDATE_POLICY_MD=1"
    );
}
