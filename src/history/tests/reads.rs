use super::*;
use crate::git_fixture::Fixture;

fn settle(owner: &mut History) {
    owner.apply(owner.updates().recv_blocking().unwrap());
}
#[test]
fn production_owner_reads_unborn_committed_and_detached_history() {
    let fixture = Fixture::new();
    let client = fixture.client();
    let identity = client.discover().unwrap();
    let mut owner = History::new(client.bind(&identity).unwrap(), identity.clone());
    settle(&mut owner);
    assert_eq!(owner.identity(), &identity);
    assert!(matches!(owner.head(), Some(Head::Unborn { .. })));
    assert!(owner.commits().is_empty());
    fixture.write("file", b"content\n");
    fixture.commit();
    owner.refresh();
    settle(&mut owner);
    assert_eq!(owner.commits().len(), 1);
    assert_eq!(owner.commits()[0].subject, b"fixture");
    fixture.run(&["checkout", "--detach", "-q"]);
    owner.refresh();
    settle(&mut owner);
    assert!(matches!(owner.head(), Some(Head::Detached { .. })));
    assert!(owner.error().is_none());
}
#[test]
fn bare_repository_history_is_read_only_and_available() {
    let fixture = Fixture::new();
    fixture.write("file", b"content\n");
    fixture.commit();
    fixture.run(&["clone", "--bare", "-q", ".", "bare.git"]);
    let client = fixture.client_at(fixture.root.path().join("bare.git"));
    let identity = client.discover().unwrap();
    assert!(identity.worktree.is_none());
    let mut owner = History::new(client.bind(&identity).unwrap(), identity);
    settle(&mut owner);
    assert_eq!(owner.commits().len(), 1);
    assert!(owner.error().is_none());
}
#[test]
fn failed_refresh_preserves_previous_authoritative_read() {
    let fixture = Fixture::new();
    fixture.write("file", b"content\n");
    fixture.commit();
    let client = fixture.client();
    let identity = client.discover().unwrap();
    let mut owner = History::new(client.bind(&identity).unwrap(), identity);
    settle(&mut owner);
    let oid = owner.commits()[0].oid.clone();
    owner.generation += 1;
    owner.busy = true;
    owner.apply(Update {
        owner: owner.token.clone(),
        generation: owner.generation,
        result: Err("controlled unavailable".into()),
        _scope: owner
            .client
            .begin_workflow(Arc::new(AtomicBool::new(false)))
            .unwrap()
            .1,
    });
    assert_eq!(owner.commits()[0].oid, oid);
    assert_eq!(owner.error(), Some("controlled unavailable"));
    owner.apply(Update {
        owner: owner.token.clone(),
        generation: owner.generation - 1,
        result: Ok((
            Head::Unborn {
                branch: "stale".into(),
            },
            vec![],
        )),
        _scope: owner
            .client
            .begin_workflow(Arc::new(AtomicBool::new(false)))
            .unwrap()
            .1,
    });
    assert_eq!(owner.commits()[0].oid, oid);
}
