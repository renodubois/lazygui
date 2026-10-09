use super::*;
use crate::git_fixture::Fixture;

fn settings(yaml: &str) -> lazygit_config::M1Settings {
    lazygit_config::Settings::load_m1(&[lazygit_config::Source {
        path: "/fixture/prefix.yml".into(),
        explicitly_chosen_global: false,
        yaml: yaml.into(),
    }])
    .unwrap()
    .m1()
    .clone()
}
fn owner(fixture: &Fixture, yaml: &str) -> Commit {
    let client = fixture.client();
    Commit::new(
        client.clone(),
        client.discover().unwrap(),
        Arc::new(git::MutationGates::new()),
        settings(yaml),
    )
}
fn prepare(owner: &mut Commit) {
    assert!(owner.prepare_draft());
    assert!(!owner.prepare_draft());
    assert!(!owner.submit());
    let update = owner.updates().recv_blocking().unwrap();
    assert!(owner.apply(update));
    assert!(!owner.busy());
    assert!(owner.error().is_none(), "{:?}", owner.error());
}
#[test]
fn configured_branch_prefix_prefills_new_draft_only_and_recall_preserves_exact_body() {
    let fixture = Fixture::new();
    fixture.run(&["symbolic-ref", "HEAD", "refs/heads/feature/AB-123"]);
    let mut owner = owner(
        &fixture,
        r#"git:
  commitPrefix:
    - {pattern: '', replace: ignored}
    - {pattern: '^bug/', replace: ignored}
    - {pattern: '^\w+\/(\w+-\w+).*', replace: '[$1] '}
"#,
    );
    prepare(&mut owner);
    assert_eq!(owner.draft().subject, "[AB-123] ");
    assert!(owner.draft().body.is_empty());
    owner.cancel();
    assert!(!owner.prepare_draft());
    owner.set_draft("", "");
    assert!(
        !owner.prepare_draft(),
        "cleared retained draft is not a new draft"
    );
    let recalled = Draft::from_message("old subject\n\nbody\n\n\nline\n");
    owner.set_draft(recalled.subject.clone(), recalled.body.clone());
    assert!(!owner.prepare_draft());
    assert_eq!(owner.draft(), &recalled);
    assert_eq!(owner.draft().body, "body\n\n\nline\n");
}
#[test]
fn repository_rules_precede_global_fallback_and_empty_match_stops() {
    let fixture = Fixture::new();
    fixture.run(&["symbolic-ref", "HEAD", "refs/heads/feature/AB-123"]);
    let name = fixture.root.path().file_name().unwrap().to_str().unwrap();
    for (repo_pattern, repo_replace, expected) in [
        ("^feature/.*", "repository ", "repository "),
        ("^bug/.*", "repository ", "global "),
        ("^feature/.*", "", ""),
    ] {
        let yaml = format!(
            "git:\n  commitPrefixes:\n    '{name}': [{{pattern: '{repo_pattern}', replace: '{repo_replace}'}}]\n  commitPrefix: [{{pattern: '^feature/.*', replace: 'global '}}]"
        );
        let mut owner = owner(&fixture, &yaml);
        prepare(&mut owner);
        assert_eq!(owner.draft().subject, expected);
        assert!(!owner.prepare_draft());
    }
    let mut nonmatching = owner(
        &fixture,
        "git: {commitPrefix: [{pattern: '^bug/', replace: ignored}]}",
    );
    prepare(&mut nonmatching);
    assert_eq!(nonmatching.draft(), &Draft::default());
    assert!(!nonmatching.prepare_draft());
}
#[test]
fn detached_full_oid_and_linked_worktree_main_repository_name() {
    let fixture = Fixture::new();
    fixture.write("file", b"content");
    fixture.commit();
    fixture.run(&["checkout", "--detach", "-q"]);
    let oid = match fixture.client().head().unwrap() {
        git::Head::Detached { oid } => oid,
        _ => panic!(),
    };
    let mut detached = owner(
        &fixture,
        "git: {commitPrefix: [{pattern: '^(.*)$', replace: '[$0] '}]}",
    );
    prepare(&mut detached);
    assert_eq!(detached.draft().subject, format!("[{oid}] "));
    let linked = tempfile::tempdir().unwrap();
    fixture.run(&[
        "worktree",
        "add",
        "-qb",
        "feature/AB-123",
        linked.path().to_str().unwrap(),
    ]);
    let client = fixture.client_at(linked.path().into());
    let name = fixture.root.path().file_name().unwrap().to_str().unwrap();
    let yaml = format!(
        "git:\n  commitPrefixes:\n    '{name}': [{{pattern: '^feature/.*', replace: 'main repo '}}]\n  commitPrefix: [{{pattern: '.*', replace: 'wrong repo '}}]"
    );
    let mut linked_owner = Commit::new(
        client.clone(),
        client.discover().unwrap(),
        Arc::new(git::MutationGates::new()),
        settings(&yaml),
    );
    prepare(&mut linked_owner);
    assert_eq!(linked_owner.draft().subject, "main repo ");
}
#[test]
fn preparation_cancellation_edits_and_recall_win_over_queued_prefix() {
    let fixture = Fixture::new();
    let yaml = "git: {commitPrefix: [{pattern: '.*', replace: prefix}]}";
    for cancel in [false, true] {
        let mut owner = owner(&fixture, yaml);
        assert!(owner.prepare_draft());
        let update = owner.updates().recv_blocking().unwrap();
        if cancel {
            owner.cancel();
        } else {
            owner.set_draft("recalled", "\nexact\nbody\n");
        }
        assert!(owner.apply(update));
        assert_eq!(owner.draft().subject, if cancel { "" } else { "recalled" });
        if !cancel {
            assert_eq!(owner.draft().body, "\nexact\nbody\n");
        }
        if cancel {
            prepare(&mut owner);
            assert_eq!(owner.draft().subject, "prefix");
        }
    }
}
#[test]
fn metadata_repository_name_fallback_and_ambiguous_linked_metadata_diagnosed() {
    let mut identity = git::Identity {
        worktree: None,
        git_dir: "/repos/bare.git".into(),
        common_dir: "/repos/bare.git".into(),
    };
    let rules = settings("git: {commitPrefixes: {example: []}}").commit_prefixes;
    assert_eq!(repository_name(&identity, &rules).unwrap(), "repos");
    identity.worktree = Some("/repos/submodule".into());
    assert_eq!(repository_name(&identity, &rules).unwrap(), "submodule");
    identity.git_dir = "/repos/.git/modules/sub/worktrees/linked".into();
    identity.common_dir = "/repos/.git/modules/sub".into();
    assert!(
        repository_name(&identity, &rules)
            .unwrap_err()
            .to_string()
            .contains("ambiguous")
    );
}
#[test]
fn failed_submission_and_explicit_retry_do_not_prepend_prefix_again() {
    let fixture = Fixture::new();
    let mut owner = owner(
        &fixture,
        "git: {commitPrefix: [{pattern: '^.*$', replace: '[ticket] '}]}",
    );
    prepare(&mut owner);
    owner.set_draft("[ticket] subject", "retained\nbody\n");
    assert!(owner.submit());
    assert!(!owner.submit());
    assert!(owner.apply(owner.updates().recv_blocking().unwrap()));
    assert!(owner.error().unwrap().contains("No changed files"));
    assert!(!owner.prepare_draft());
    assert_eq!(owner.draft().subject, "[ticket] subject");
    assert_eq!(owner.draft().body, "retained\nbody\n");
    fixture.write("file", b"content");
    fixture.run(&["add", "file"]);
    assert!(owner.submit());
    assert!(owner.apply(owner.updates().recv_blocking().unwrap()));
    assert!(matches!(owner.outcome(), Some(Outcome::Committed { .. })));
    assert_eq!(
        fixture.client().history(1).unwrap()[0].message,
        b"[ticket] subject\n\nretained\nbody\n"
    );
}
#[test]
fn successful_commit_resets_prefix_preparation_for_next_new_draft() {
    let fixture = Fixture::new();
    fixture.write("file", b"content");
    fixture.run(&["add", "file"]);
    let mut owner = owner(
        &fixture,
        "git: {commitPrefix: [{pattern: '^.*$', replace: prefix}]}",
    );
    prepare(&mut owner);
    owner.set_draft("prefix subject", "body\n");
    assert!(owner.submit());
    assert!(owner.apply(owner.updates().recv_blocking().unwrap()));
    assert!(matches!(owner.outcome(), Some(Outcome::Committed { .. })));
    prepare(&mut owner);
    assert_eq!(owner.draft().subject, "prefix");
}
