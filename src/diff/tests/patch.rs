use super::*;
use crate::{
    git::{Client, Side},
    git_fixture::Fixture,
};
fn apply(client: &Client, patch: &Patch, selection: &[usize], reverse: bool) {
    match patch
        .select(&selection.iter().copied().collect(), reverse)
        .unwrap()
    {
        Selection::Patch(bytes) => client.apply_index(&bytes).unwrap(),
        Selection::WholeFile => panic!("unexpected fallback"),
    }
}
#[test]
fn selective_replacement_addition_deletion_and_reverse_preserve_worktree() {
    let f = Fixture::new();
    f.write("file", b"one\ntwo\nthree\nfour\nfive\n");
    f.commit();
    let worktree = b"one\nTWO\nthree\nfive\nsix\n";
    f.write("file", worktree);
    let client = f.client();
    let patch = Patch::parse(&client.diff(Path::new("file"), Side::Worktree).unwrap()).unwrap();
    apply(&client, &patch, &[0, 1], false);
    assert_eq!(f.run(&["show", ":file"]), b"one\nTWO\nthree\nfour\nfive\n");
    let staged = Patch::parse(&client.diff(Path::new("file"), Side::Index).unwrap()).unwrap();
    apply(&client, &staged, &[0, 1], true);
    assert_eq!(f.run(&["show", ":file"]), b"one\ntwo\nthree\nfour\nfive\n");
    let patch = Patch::parse(&client.diff(Path::new("file"), Side::Worktree).unwrap()).unwrap();
    apply(&client, &patch, &[2], false);
    assert_eq!(f.run(&["show", ":file"]), b"one\ntwo\nthree\nfive\n");
    let patch = Patch::parse(&client.diff(Path::new("file"), Side::Worktree).unwrap()).unwrap();
    apply(&client, &patch, &[2], false); // remaining insertion after replacement pair
    assert_eq!(f.run(&["show", ":file"]), b"one\ntwo\nthree\nfive\nsix\n");
    assert_eq!(std::fs::read(f.root.path().join("file")).unwrap(), worktree);
}
use std::path::Path;
#[test]
fn complete_add_delete_rename_use_file_semantics_and_stale_snapshot_rejects() {
    let f = Fixture::new();
    f.write("file", b"one\n");
    f.commit();
    std::fs::remove_file(f.root.path().join("file")).unwrap();
    let client = f.client();
    let raw = client.diff(Path::new("file"), Side::Worktree).unwrap();
    let patch = Patch::parse(&raw).unwrap();
    assert_eq!(
        patch
            .select(&(0..patch.changes()).collect(), false)
            .unwrap(),
        Selection::WholeFile
    );
    assert!(!patch.matches_snapshot(b"replacement"));
    client.stage_file(Path::new("file")).unwrap();
    assert_eq!(client.status().unwrap()[0].index, b'D');
    client.unstage_file(Path::new("file")).unwrap();
    assert_eq!(client.status().unwrap()[0].worktree, b'D');
    f.write("new", b"new\n");
    let patch = Patch::parse(&client.untracked_diff(Path::new("new")).unwrap()).unwrap();
    assert_eq!(
        patch
            .select(&(0..patch.changes()).collect(), false)
            .unwrap(),
        Selection::WholeFile
    );
    client.stage_file(Path::new("new")).unwrap();
    let patch = Patch::parse(&client.diff(Path::new("new"), Side::Index).unwrap()).unwrap();
    assert_eq!(
        patch.select(&(0..patch.changes()).collect(), true).unwrap(),
        Selection::WholeFile
    );
    client.unstage_file(Path::new("new")).unwrap();
    assert!(
        client
            .status()
            .unwrap()
            .iter()
            .any(|e| e.path == "new" && e.index == b'?')
    );
    f.write("file", b"one\n");
    f.run(&["mv", "file", "renamed"]);
    client.unstage_file(Path::new("file")).unwrap();
    client.unstage_file(Path::new("renamed")).unwrap();
    client.stage_file(Path::new("file")).unwrap();
    client.stage_file(Path::new("renamed")).unwrap();
    assert!(client.status().unwrap().iter().any(|e| e.index == b'R'));
}
#[test]
fn binary_mode_and_missing_newline_are_explicit_fallbacks() {
    for bytes in [
        b"Binary files a and b differ\n".as_slice(),
        b"diff --git a/a b/a\nold mode 100644\nnew mode 100755\n",
        b"diff --git a/a b/a\n@@ -1 +1 @@\n-a\n+b\n\\ No newline at end of file\n",
    ] {
        assert!(Patch::parse(bytes).is_err());
    }
}
