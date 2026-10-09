use super::*;
use crate::git_fixture::Fixture;
#[test]
fn discovers_unborn_bare_linked_and_submodule_and_byte_paths() {
    let f = Fixture::new();
    let id = f.client().discover().unwrap();
    assert_eq!(id.worktree.as_deref(), Some(f.root.path()));
    assert!(f.client().status().unwrap().is_empty());
    f.write("unborn", b"unborn\\n");
    f.client().stage_file(Path::new("unborn")).unwrap();
    f.client().unstage_file(Path::new("unborn")).unwrap();
    assert!(
        f.client()
            .status()
            .unwrap()
            .iter()
            .any(|entry| entry.path == "unborn" && entry.index == b'?')
    );
    std::fs::remove_file(f.root.path().join("unborn")).unwrap();
    f.write("file", b"one\n");
    f.commit();
    f.run(&["worktree", "add", "-qb", "linked", "linked"]);
    let linked = f
        .client_at(f.root.path().join("linked"))
        .discover()
        .unwrap();
    assert_ne!(linked.git_dir, id.git_dir);
    assert_eq!(linked.common_dir, id.common_dir);
    f.run(&["init", "--bare", "-q", "bare"]);
    assert!(
        f.client_at(f.root.path().join("bare"))
            .discover()
            .unwrap()
            .worktree
            .is_none()
    );
    f.run(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        "-q",
        "./",
        "sub",
    ]);
    assert!(
        f.client_at(f.root.path().join("sub"))
            .discover()
            .unwrap()
            .git_dir
            .ends_with(".git/modules/sub")
    );
    let path = OsString::from_vec(b"-space \t\n\xff".to_vec());
    std::fs::write(f.root.path().join(&path), b"content").unwrap();
    let entries = f.client().status().unwrap();
    assert!(entries.iter().any(|entry| entry.path == path));
    f.client().stage_file(Path::new(&path)).unwrap();
    assert!(
        f.client()
            .status()
            .unwrap()
            .iter()
            .any(|e| e.path == path && e.index == b'A')
    );
    f.run(&["mv", "file", "renamed"]);
    let entries = f.client().status().unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.path == "renamed" && e.original.as_deref() == Some(OsStr::new("file")))
    );
}
#[test]
fn malformed_rename_and_nonrepo_are_errors() {
    assert!(decode_status(b"R  target\0").is_err());
    assert!(
        Fixture::new()
            .client_at(tempfile::tempdir().unwrap().path().to_owned())
            .discover()
            .is_err()
    );
}
#[test]
fn controlled_executor_receives_typed_read_binding() {
    struct Fake;
    impl Executor for Fake {
        fn execute(
            &self,
            command: Command,
            _: Vec<u8>,
            _: Arc<AtomicBool>,
        ) -> io::Result<process::Output> {
            assert_eq!(command.get_current_dir(), Some(Path::new("/fixture")));
            assert!(
                command
                    .get_envs()
                    .any(|(key, value)| key == "GIT_OPTIONAL_LOCKS"
                        && value == Some(OsStr::new("0")))
            );
            Ok(process::Output {
                code: Some(0),
                stdout: b" M file\0".to_vec(),
                stderr: vec![],
                cancelled: false,
                truncated: false,
            })
        }
    }
    assert_eq!(
        Client::with_executor("/fixture".into(), Arc::new(Fake))
            .status()
            .unwrap()[0]
            .path,
        "file"
    );
}
