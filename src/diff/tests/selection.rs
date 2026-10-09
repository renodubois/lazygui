use super::*;
use crate::{
    git::{Client, Side},
    git_fixture::Fixture,
};
use std::{os::unix::fs::PermissionsExt, path::Path};

fn apply(client: &Client, patch: &Patch, ids: &BTreeSet<ChangeId>, reverse: bool) {
    let Selection::Patch(bytes) = patch.select(ids, reverse).unwrap() else {
        panic!("unexpected whole-file fallback");
    };
    client.apply_index(&bytes).unwrap();
}

fn subset(mask: usize, count: usize) -> BTreeSet<ChangeId> {
    (0..count).filter(|id| mask & (1 << id) != 0).collect()
}

#[test]
fn every_partial_added_deleted_subset_in_both_directions_preserves_file_semantics() {
    let lines = [b"first\n".as_slice(), b"second\n", b"third\n"];
    let contents = lines.concat();
    for added in [false, true] {
        for executable in [false, true] {
            let f = Fixture::new();
            f.write("anchor", b"anchor\n");
            if !added {
                f.write("file", &contents);
                std::fs::set_permissions(
                    f.root.path().join("file"),
                    std::fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
                )
                .unwrap();
            }
            f.commit();
            if added {
                f.write("file", &contents);
                std::fs::set_permissions(
                    f.root.path().join("file"),
                    std::fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
                )
                .unwrap();
            } else {
                std::fs::remove_file(f.root.path().join("file")).unwrap();
            }
            let client = f.client();
            for reverse in [false, true] {
                for mask in 1..7 {
                    f.run(&["read-tree", "HEAD"]);
                    if reverse {
                        client.stage_file(Path::new("file")).unwrap();
                    }
                    let raw = if added && !reverse {
                        client.untracked_diff(Path::new("file")).unwrap()
                    } else {
                        client
                            .diff(
                                Path::new("file"),
                                if reverse { Side::Index } else { Side::Worktree },
                            )
                            .unwrap()
                    };
                    let patch = Patch::parse(&raw).unwrap();
                    assert_eq!(patch.changes(), 3);
                    assert!(patch.hunks().all(|h| h.supports_partial_selection));
                    assert_eq!(
                        patch.select(&(0..3).collect(), reverse).unwrap(),
                        Selection::WholeFile
                    );
                    let ids = subset(mask, 3);
                    apply(&client, &patch, &ids, reverse);
                    let fresh_raw = client
                        .diff(
                            Path::new("file"),
                            if reverse { Side::Index } else { Side::Worktree },
                        )
                        .unwrap();
                    let fresh = Patch::parse(&fresh_raw).unwrap();
                    let map = patch
                        .remap_changes(Some(&fresh), Some((&ids, reverse)))
                        .unwrap();
                    for (old, new) in map.into_iter().enumerate() {
                        assert_eq!(new.is_none(), ids.contains(&old));
                    }
                    // Creation keeps chosen lines; partial deletion keeps the complement.
                    let creates = added != reverse;
                    let expected: Vec<_> = lines
                        .iter()
                        .enumerate()
                        .filter(|(id, _)| ids.contains(id) == creates)
                        .flat_map(|(_, line)| line.iter().copied())
                        .collect();
                    assert_eq!(
                        f.run(&["show", ":file"]),
                        expected,
                        "added={added} reverse={reverse} mask={mask}"
                    );
                    let mode = if executable {
                        b"100755".as_slice()
                    } else {
                        b"100644"
                    };
                    assert!(
                        f.run(&["ls-files", "--stage", "--", "file"])
                            .starts_with(mode)
                    );
                    if added {
                        assert_eq!(std::fs::read(f.root.path().join("file")).unwrap(), contents);
                    } else {
                        assert!(!f.root.path().join("file").exists());
                    }
                }
            }
        }
    }
}

#[test]
fn all_multi_hunk_subsets_apply_with_source_coordinates_and_reverse_deltas() {
    let f = Fixture::new();
    let original: Vec<Vec<u8>> = (0..30)
        .map(|i| format!("line {i}\n").into_bytes())
        .collect();
    f.write("file", &original.concat());
    f.commit();
    let mut changed = original.clone();
    changed[0] = b"replacement\n".to_vec();
    changed.remove(14);
    changed.push(b"tail\n".to_vec());
    let worktree = changed.concat();
    f.write("file", &worktree);
    let client = f.client();
    for reverse in [false, true] {
        for mask in 1..16 {
            f.run(&["read-tree", "HEAD"]);
            if reverse {
                client.stage_file(Path::new("file")).unwrap();
            }
            // Rich index metadata, abbreviated counts, and multiple hunks are Git-generated.
            let raw = client
                .diff(
                    Path::new("file"),
                    if reverse { Side::Index } else { Side::Worktree },
                )
                .unwrap();
            let patch = Patch::parse(&raw).unwrap();
            assert_eq!(patch.changes(), 4);
            assert_eq!(patch.hunks().count(), 3);
            let ids = subset(mask, 4);
            apply(&client, &patch, &ids, reverse);
            let fresh_raw = client
                .diff(
                    Path::new("file"),
                    if reverse { Side::Index } else { Side::Worktree },
                )
                .unwrap();
            let fresh = (!fresh_raw.is_empty()).then(|| Patch::parse(&fresh_raw).unwrap());
            let map = patch
                .remap_changes(fresh.as_ref(), Some((&ids, reverse)))
                .unwrap();
            assert_eq!(
                map.iter().filter(|id| id.is_some()).count(),
                patch.changes() - ids.len()
            );
            for (old, new) in map.into_iter().enumerate() {
                assert_eq!(new.is_none(), ids.contains(&old));
            }
            let mut expected = Vec::new();
            if ids.contains(&0) == reverse {
                expected.extend_from_slice(&original[0]);
            }
            if ids.contains(&1) != reverse {
                expected.extend_from_slice(b"replacement\n");
            }
            for (i, line) in original.iter().enumerate().skip(1) {
                if i != 14 || ids.contains(&2) == reverse {
                    expected.extend_from_slice(line);
                }
            }
            if ids.contains(&3) != reverse {
                expected.extend_from_slice(b"tail\n");
            }
            assert_eq!(
                f.run(&["show", ":file"]),
                expected,
                "reverse={reverse} mask={mask}"
            );
            assert_eq!(std::fs::read(f.root.path().join("file")).unwrap(), worktree);
        }
    }
}

#[test]
fn display_mapping_exposes_snapshot_scoped_hunks_lines_and_change_ordinals() {
    let f = Fixture::new();
    let original = (0..20).map(|i| format!("line {i}\n")).collect::<String>();
    f.write("file", original.as_bytes());
    f.commit();
    f.write(
        "file",
        original
            .replace("line 1\n", "replacement\n")
            .replace("line 18\n", "")
            .as_bytes(),
    );
    let raw = f.client().diff(Path::new("file"), Side::Worktree).unwrap();
    let patch = Patch::parse(&raw).unwrap();
    let hunks: Vec<_> = patch.hunks().collect();
    let lines: Vec<_> = patch.lines().collect();
    assert_eq!(hunks.len(), 2);
    assert_eq!(hunks[0].id, 0);
    assert_eq!(hunks[0].changes, 0..2);
    assert_eq!(hunks[1].id, 1);
    assert_eq!(hunks[1].changes, 2..3);
    assert_eq!(hunks[0].lines.start, 0);
    assert_eq!(hunks[0].lines.end, hunks[1].lines.start);
    assert_eq!(hunks[1].lines.end, lines.len());
    assert_eq!(
        lines.iter().filter_map(|l| l.change_id).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    for (id, line) in lines.iter().enumerate() {
        assert_eq!(line.id, id);
        let hunk = &hunks[line.hunk_id];
        assert!(hunk.lines.contains(&id));
        assert!(raw.windows(line.bytes.len()).any(|w| w == line.bytes));
        if let Some(change) = line.change_id {
            assert!(hunk.changes.contains(&change));
        }
    }
    let deleted = lines.iter().find(|l| l.change_id == Some(0)).unwrap();
    assert_eq!(
        (deleted.kind, deleted.old_line, deleted.new_line),
        (LineKind::Deletion, Some(2), None)
    );
    let added = lines.iter().find(|l| l.change_id == Some(1)).unwrap();
    assert_eq!(
        (added.kind, added.old_line, added.new_line),
        (LineKind::Addition, None, Some(2))
    );
    assert!(patch.matches_snapshot(&raw));
    assert!(!patch.matches_snapshot(&[raw.as_slice(), b"\n"].concat()));
    // A hunk's change range can go straight to select; screen offsets are never used.
    apply(
        &f.client(),
        &patch,
        &hunks[1].changes.clone().collect(),
        false,
    );
    assert_eq!(
        f.run(&["show", ":file"]),
        original.replace("line 18\n", "").as_bytes()
    );
}

#[test]
fn quoted_byte_paths_content_and_non_utf8_function_headers_are_not_lossy() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let f = Fixture::new();
    let path = Path::new("-space tab\tnewline\n120000").join(OsString::from_vec(b"\xff".to_vec()));
    std::fs::create_dir_all(f.root.path().join(path.parent().unwrap())).unwrap();
    let original = b"120000\r\nold mode 100755\r\nBinary files unchanged\r\nold\xff\r\nend\r\n";
    let worktree = b"120000\r\nold mode 100755\r\nBinary files unchanged\r\nnew\xfe\r\nend\r\n";
    std::fs::write(f.root.path().join(&path), original).unwrap();
    f.commit();
    std::fs::write(f.root.path().join(&path), worktree).unwrap();
    let client = f.client();
    let mut raw = client.diff(&path, Side::Worktree).unwrap();
    let header_end = raw.windows(3).position(|w| w == b"@@\n").unwrap();
    raw.splice(
        header_end + 2..header_end + 2,
        b" function \xff".iter().copied(),
    );
    let patch = Patch::parse(&raw).unwrap();
    assert!(
        patch
            .hunks()
            .next()
            .unwrap()
            .header
            .ends_with(b" function \xff\n")
    );
    apply(&client, &patch, &BTreeSet::from([0, 1]), false);
    let staged = Patch::parse(&client.diff(&path, Side::Index).unwrap()).unwrap();
    apply(&client, &staged, &BTreeSet::from([0, 1]), true);
    assert!(client.diff(&path, Side::Index).unwrap().is_empty());
    assert_eq!(std::fs::read(f.root.path().join(path)).unwrap(), worktree);
}

#[test]
fn zero_context_normal_line_selection_is_explicitly_blocked_without_mutation() {
    let f = Fixture::new();
    f.write("file", b"one\ntwo\nthree\n");
    f.commit();
    f.write("file", b"ONE\ntwo\nTHREE\n");
    let raw = f.run(&[
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--unified=0",
        "--",
        "file",
    ]);
    let patch = Patch::parse(&raw).unwrap();
    assert!(
        patch
            .hunks()
            .all(|h| !h.has_context && !h.supports_partial_selection)
    );
    for reverse in [false, true] {
        for mask in 1..16 {
            let error = patch.select(&subset(mask, 4), reverse).unwrap_err();
            assert!(error.to_string().contains("zero-context"));
        }
    }
    assert_eq!(f.run(&["show", ":file"]), b"one\ntwo\nthree\n");
    assert!(patch.select(&BTreeSet::new(), false).is_err());
    assert!(
        patch
            .select(&BTreeSet::from([patch.changes()]), false)
            .is_err()
    );
}

#[test]
fn missing_final_newline_has_explicit_fallback_and_whole_file_preserves_bytes() {
    let f = Fixture::new();
    f.write("file", b"old\nlast");
    f.commit();
    let worktree = b"new\nlast";
    f.write("file", worktree);
    let client = f.client();
    let raw = client.diff(Path::new("file"), Side::Worktree).unwrap();
    let error = Patch::parse(&raw).unwrap_err();
    assert!(error.to_string().contains("missing final newline"));
    client.stage_file(Path::new("file")).unwrap();
    assert_eq!(f.run(&["show", ":file"]), worktree);
    client.unstage_file(Path::new("file")).unwrap();
    assert_eq!(f.run(&["show", ":file"]), b"old\nlast");
    assert_eq!(std::fs::read(f.root.path().join("file")).unwrap(), worktree);
}

#[test]
fn installed_git_binary_symlink_gitlink_mode_and_rename_remain_fallbacks() {
    let f = Fixture::new();
    f.write("file", b"one\ntwo\n");
    f.commit();
    let client = f.client();
    f.write("file", b"one\0two\n");
    assert!(Patch::parse(&client.diff(Path::new("file"), Side::Worktree).unwrap()).is_err());
    f.write("file", b"one\ntwo\n");
    std::fs::set_permissions(
        f.root.path().join("file"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert!(Patch::parse(&client.diff(Path::new("file"), Side::Worktree).unwrap()).is_err());
    std::fs::set_permissions(
        f.root.path().join("file"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    std::os::unix::fs::symlink("file", f.root.path().join("link")).unwrap();
    client.stage_file(Path::new("link")).unwrap();
    assert!(Patch::parse(&client.diff(Path::new("link"), Side::Index).unwrap()).is_err());
    let hash = String::from_utf8(f.run(&["rev-parse", "HEAD"])).unwrap();
    f.run(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{},submodule", hash.trim()),
    ]);
    assert!(Patch::parse(&client.diff(Path::new("submodule"), Side::Index).unwrap()).is_err());
    f.run(&["mv", "file", "renamed"]);
    let renamed = f.run(&[
        "diff",
        "--cached",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--",
        "file",
        "renamed",
    ]);
    assert!(renamed.windows(12).any(|w| w == b"rename from "));
    assert!(Patch::parse(&renamed).is_err());
}

#[test]
fn anchor_mapping_rejects_external_bytes_coordinates_and_context_even_with_duplicate_lines() {
    let original = b"diff --git a/file b/file\nindex abc..def 100644\n--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n-same\n+first\n context\n tail\n@@ -20,3 +20,3 @@\n-same\n+second\n context\n tail\n";
    let residual = b"diff --git a/file b/file\nindex def..abc 100644\n--- a/file\n+++ b/file\n@@ -20,3 +20,3 @@\n-same\n+second\n context\n tail\n";
    let old = Patch::parse(original).unwrap();
    let selected = [0, 1].into_iter().collect();
    let fresh = Patch::parse(residual).unwrap();
    assert_eq!(
        old.remap_changes(Some(&fresh), Some((&selected, false)))
            .unwrap(),
        vec![None, None, Some(0), Some(1)]
    );
    let text = std::str::from_utf8(residual).unwrap();
    for external in [
        text.replace("second", "first"),
        text.replace("-20,3 +20,3", "-19,3 +19,3"),
        text.replace(" context", " external context"),
        text.replace("-same", "-different"),
    ] {
        assert!(
            old.remap_changes(
                Some(&Patch::parse(external.as_bytes()).unwrap()),
                Some((&selected, false))
            )
            .is_err()
        );
    }
}

#[test]
fn malformed_ranges_counts_metadata_and_multiple_files_are_rejected() {
    let valid = b"diff --git a/file b/file\nindex abc..def 100644\n--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n-old\n+new\n context\n";
    assert!(Patch::parse(valid).is_ok());
    let text = std::str::from_utf8(valid).unwrap();
    for malformed in [
        text.replace("-1,2", "-1,3"),
        text.replace("-1,2", "-0,2"),
        text.replace("-1,2", "-18446744073709551615,2"),
        text.replace("+1,2", "-1,2"),
        text.replace("+1,2", "+2,2"),
        text.replace("@@\n", "@\n"),
        text.replace("100644", "120000"),
        text.replace("100644", "160000"),
        text.replace(
            "index abc..def 100644\n",
            "old mode 100644\nnew mode 100755\n",
        ),
        text.replace("index abc..def 100644\n", "copy from file\ncopy to other\n"),
        text.replace(
            "index abc..def 100644\n",
            "similarity index 90%\nrename from file\nrename to other\n",
        ),
        text.replace("index abc..def 100644\n", "GIT binary patch\n"),
        text.replace("index abc..def 100644\n", "unknown metadata\n"),
        text.replace(
            "index abc..def 100644\n",
            "index abc..def 100644\nindex abc..def 100644\n",
        ),
        text.replace("+++ b/file\n", "+++ b/file\n+++ b/other\n"),
        format!("{text}{text}"),
        format!("{text}@@ -1,2 +1,2 @@\n-old\n+new\n context\n"),
    ] {
        assert!(
            Patch::parse(malformed.as_bytes()).is_err(),
            "accepted {malformed:?}"
        );
    }
    assert!(Patch::parse(&valid[..valid.len() - 1]).is_err());
}
