use super::*;
use crate::git_fixture::Fixture;

#[test]
fn canonical_aliases_share_worktree_and_leases_cover_reconciliation() {
    let f = Fixture::new();
    let gates = MutationGates::new();
    let id = f.client().discover().unwrap();
    let alias_dir = tempfile::tempdir().unwrap();
    let alias = alias_dir.path().join("alias");
    std::os::unix::fs::symlink(f.root.path(), &alias).unwrap();
    let mut alias_id = id.clone();
    alias_id.worktree = Some(alias.clone());
    alias_id.common_dir = alias.join(".git");
    let lease = gates
        .acquire(&id, MutationScope::Worktree, &AtomicBool::new(false))
        .unwrap();
    assert!(lease.covers(&alias_id, MutationScope::Worktree).unwrap());
    assert!(!lease.covers(&id, MutationScope::All).unwrap());
    f.write("file", b"first\n");
    f.client().stage_file(std::path::Path::new("file")).unwrap();
    assert!(
        gates
            .clone()
            .try_acquire(&alias_id, MutationScope::Worktree)
            .unwrap()
            .is_none()
    );
    assert_eq!(f.client().status().unwrap()[0].index, b'A');
    // Dispatch does not release the owner lease; only after reconciliation above.
    drop(lease);
    assert!(
        gates
            .try_acquire(&alias_id, MutationScope::Worktree)
            .unwrap()
            .is_some()
    );
}

#[test]
fn linked_worktrees_share_common_gate_but_not_index_gate_and_all_is_atomic() {
    let f = Fixture::new();
    f.write("file", b"one\n");
    f.commit();
    f.run(&["worktree", "add", "-qb", "linked", "linked"]);
    let id = f.client().discover().unwrap();
    let linked = f
        .client_at(f.root.path().join("linked"))
        .discover()
        .unwrap();
    let gates = MutationGates::new();
    let index = gates
        .try_acquire(&id, MutationScope::Worktree)
        .unwrap()
        .unwrap();
    let linked_index = gates
        .try_acquire(&linked, MutationScope::Worktree)
        .unwrap()
        .unwrap();
    assert!(
        gates
            .try_acquire(&id, MutationScope::All)
            .unwrap()
            .is_none()
    );
    // Failed All acquisition must not reserve its common-dir component.
    let shared = gates
        .try_acquire(&linked, MutationScope::Shared)
        .unwrap()
        .unwrap();
    assert!(
        gates
            .try_acquire(&id, MutationScope::Shared)
            .unwrap()
            .is_none()
    );
    drop((index, linked_index, shared));
    let all = gates.try_acquire(&id, MutationScope::All).unwrap().unwrap();
    assert!(all.covers(&id, MutationScope::All).unwrap());
    assert!(
        gates
            .try_acquire(&linked, MutationScope::All)
            .unwrap()
            .is_none()
    );
    assert!(
        gates
            .try_acquire(&linked, MutationScope::Worktree)
            .unwrap()
            .is_some()
    );
}

#[test]
fn different_worktrees_bound_to_same_git_dir_contend_on_the_index() {
    let f = Fixture::new();
    let other = tempfile::tempdir().unwrap();
    let id = f.client().discover().unwrap();
    let mut alternate = id.clone();
    alternate.worktree = Some(other.path().to_owned());
    let gates = MutationGates::new();
    let lease = gates
        .try_acquire(&id, MutationScope::Worktree)
        .unwrap()
        .unwrap();
    assert!(
        gates
            .try_acquire(&alternate, MutationScope::Worktree)
            .unwrap()
            .is_none()
    );
    drop(lease);
    assert!(
        gates
            .try_acquire(&alternate, MutationScope::Worktree)
            .unwrap()
            .is_some()
    );
}

#[test]
fn waits_cancel_and_bare_rejects_worktree_writes() {
    let f = Fixture::new();
    let gates = MutationGates::new();
    let id = f.client().discover().unwrap();
    let _lease = gates
        .try_acquire(&id, MutationScope::Worktree)
        .unwrap()
        .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let waiting = {
        let gates = gates.clone();
        let id = id.clone();
        let cancel = cancel.clone();
        std::thread::spawn(move || gates.acquire(&id, MutationScope::Worktree, &cancel))
    };
    cancel.store(true, Ordering::Release);
    assert_eq!(
        waiting.join().unwrap().err().unwrap().kind(),
        io::ErrorKind::Interrupted
    );
    f.run(&["init", "--bare", "-q", "bare"]);
    let bare = f.client_at(f.root.path().join("bare")).discover().unwrap();
    assert!(gates.try_acquire(&bare, MutationScope::Worktree).is_err());
    assert!(gates.try_acquire(&bare, MutationScope::All).is_err());
    assert!(
        gates
            .try_acquire(&bare, MutationScope::Shared)
            .unwrap()
            .is_some()
    );
}
