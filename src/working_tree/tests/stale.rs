use super::*;
use crate::{diff::Patch, git::Side, git_fixture::Fixture};
#[test]
fn stale_target_never_dispatches_write_and_reverse_selection_preserves_other_staged_changes() {
    let f = Fixture::new();
    f.write("file", b"a\nb\nc\n");
    f.commit();
    f.write("file", b"A\nb\nC\n");
    let client = f.client();
    let path = std::path::Path::new("file");
    let snapshot = Patch::parse(&client.diff(path, Side::Worktree).unwrap()).unwrap();
    f.write("file", b"unrelated replacement\n");
    assert!(
        apply_selection(
            &client,
            path,
            Side::Worktree,
            &snapshot,
            &[0, 1].into_iter().collect()
        )
        .is_err()
    );
    assert_eq!(f.run(&["show", ":file"]), b"a\nb\nc\n");
    f.write("file", b"A\nb\nC\n");
    client.stage_file(path).unwrap();
    let snapshot = Patch::parse(&client.diff(path, Side::Index).unwrap()).unwrap();
    apply_selection(
        &client,
        path,
        Side::Index,
        &snapshot,
        &[0, 1].into_iter().collect(),
    )
    .unwrap();
    assert_eq!(f.run(&["show", ":file"]), b"a\nb\nC\n");
    assert_eq!(
        std::fs::read(f.root.path().join("file")).unwrap(),
        b"A\nb\nC\n"
    );
}
