use super::*;
use crate::{diff::Patch, git::Side, git_fixture::Fixture};
#[test]
fn rapid_actions_wait_for_refresh_and_resolve_the_next_surviving_change() {
    let f = Fixture::new();
    f.write("file", b"a\nb\nc\n");
    f.commit();
    f.write("file", b"A\nb\nC\n");
    let client = f.client();
    let mut barrier = Barrier::default();
    barrier.enqueue();
    barrier.enqueue();
    for expected in [b"A\nb\nc\n".as_slice(), b"A\nb\nC\n"] {
        let generation = barrier.begin().unwrap();
        assert!(barrier.begin().is_none());
        assert!(!barrier.reconciled(generation + 1));
        let raw = client
            .diff(std::path::Path::new("file"), Side::Worktree)
            .unwrap();
        let patch = Patch::parse(&raw).unwrap();
        assert!(patch.matches_snapshot(&raw));
        apply_selection(
            &client,
            std::path::Path::new("file"),
            Side::Worktree,
            &patch,
            &[0, 1].into_iter().collect(),
        )
        .unwrap();
        // Refresh before releasing next intention, not a cached patch target.
        let _fresh = client
            .diff(std::path::Path::new("file"), Side::Worktree)
            .unwrap();
        assert_eq!(f.run(&["show", ":file"]), expected);
        assert!(barrier.reconciled(generation));
    }
    assert!(barrier.begin().is_none());
}
