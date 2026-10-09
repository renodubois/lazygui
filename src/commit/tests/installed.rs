//! Real installed Git and disposable executable hooks, never a user provider.
use super::*;
use crate::{
    git::process::{Executor, Native, Output},
    git_fixture::Fixture,
};
use std::{
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

struct WithHooks {
    root: PathBuf,
}
impl Executor for WithHooks {
    fn execute(
        &self,
        command: Command,
        input: Vec<u8>,
        cancel: Arc<AtomicBool>,
    ) -> io::Result<Output> {
        let mut isolated = Command::new("/usr/bin/git");
        isolated
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.root.join("home"))
            .env("XDG_CONFIG_HOME", self.root.join("home"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_AUTHOR_NAME", "Fixture Author")
            .env("GIT_AUTHOR_EMAIL", "author@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture Committer")
            .env("GIT_COMMITTER_EMAIL", "committer@example.invalid")
            .env("GIT_CONFIG_COUNT", "4")
            .env("GIT_CONFIG_KEY_0", "core.hooksPath")
            .env("GIT_CONFIG_VALUE_0", &self.root)
            .env("GIT_CONFIG_KEY_1", "commit.gpgSign")
            .env("GIT_CONFIG_VALUE_1", "false")
            .env("GIT_CONFIG_KEY_2", "tag.gpgSign")
            .env("GIT_CONFIG_VALUE_2", "false")
            .env("GIT_CONFIG_KEY_3", "credential.helper")
            .env("GIT_CONFIG_VALUE_3", "")
            .args(command.get_args())
            .current_dir(command.get_current_dir().unwrap());
        if let Some((_, Some(value))) = command
            .get_envs()
            .find(|(key, _)| *key == "GIT_OPTIONAL_LOCKS")
        {
            isolated.env("GIT_OPTIONAL_LOCKS", value);
        }
        Native.execute(isolated, input, cancel)
    }
}
struct Hooks {
    repo: Fixture,
    root: tempfile::TempDir,
    client: git::Client,
}
impl Hooks {
    fn new() -> Self {
        let repo = Fixture::new();
        let root = tempfile::tempdir().unwrap();
        let client = git::Client::with_executor(
            repo.root.path().into(),
            Arc::new(WithHooks {
                root: root.path().into(),
            }),
        );
        repo.write("file", b"staged content\n");
        client.stage_file(Path::new("file")).unwrap();
        Self { repo, root, client }
    }
    fn install(&self, name: &str, script: &str) {
        let path = self.root.path().join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    fn marker(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }
    fn owner(&self, settings: Settings) -> Commit {
        Commit::new(
            self.client.clone(),
            self.client.discover().unwrap(),
            Arc::new(git::MutationGates::new()),
            settings,
        )
    }
}
fn settings() -> Settings {
    let mut settings = Settings::from(lazygit_config::M1Settings::default());
    settings.skip_hook_prefix.clear();
    settings
}
fn settle(owner: &mut Commit) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while owner.busy() {
        if let Ok(update) = owner.updates().try_recv() {
            assert!(owner.apply(update));
        } else {
            assert!(Instant::now() < deadline, "commit worker did not settle");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
#[test]
fn installed_hook_failure_and_explicit_corrected_retry_reconcile_actual_state() {
    let hooks = Hooks::new();
    hooks.install(
        "pre-commit",
        "printf 'disposable hook refused\\n' >&2; exit 1",
    );
    let mut owner = hooks.owner(settings());
    owner.set_draft("subject", "original body");
    assert!(owner.submit());
    assert!(!owner.submit());
    settle(&mut owner);
    assert!(
        matches!(owner.outcome(), Some(Outcome::NotCommitted { head: git::Head::Unborn { .. }, status }) if status.len() == 1 && status[0].index == b'A')
    );
    assert!(owner.error().unwrap().contains("disposable hook refused"));
    assert_eq!(owner.draft().body, "original body");
    assert!(hooks.client.history(10).unwrap().is_empty());
    hooks.install("pre-commit", "exit 0");
    owner.set_draft("corrected", "retained body\n");
    assert!(owner.submit());
    settle(&mut owner);
    assert!(
        matches!(owner.outcome(), Some(Outcome::Committed { head, status }) if head == &hooks.client.head().unwrap() && status.is_empty())
    );
    assert_eq!(owner.draft(), &Draft::default());
    assert!(owner.error().is_none());
    let records = hooks.client.history(10).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].message, b"corrected\n\nretained body\n");
    assert_eq!(
        std::fs::read(hooks.repo.root.path().join("file")).unwrap(),
        b"staged content\n"
    );
}
#[test]
fn git_signoff_uses_real_committer_identity_and_preserves_ordinary_hooks() {
    let hooks = Hooks::new();
    hooks.install(
        "pre-commit",
        "printf ran > \"$(git config core.hooksPath)/pre-ran\"",
    );
    hooks.install("commit-msg", "grep -q '^Signed-off-by: Fixture Committer <committer@example.invalid>$' \"$1\" || exit 1\nprintf ran > \"$(git config core.hooksPath)/msg-ran\"");
    let mut settings = settings();
    settings.message.sign_off = true;
    let mut owner = hooks.owner(settings);
    owner.set_draft("subject", "body");
    assert!(owner.submit());
    settle(&mut owner);
    assert!(matches!(owner.outcome(), Some(Outcome::Committed { .. })));
    assert!(hooks.marker("pre-ran").exists());
    assert!(hooks.marker("msg-ran").exists());
    let records = hooks.client.history(10).unwrap();
    assert_eq!(records[0].author, b"Fixture Author");
    assert_eq!(
        records[0].message,
        b"subject\n\nbody\n\nSigned-off-by: Fixture Committer <committer@example.invalid>\n"
    );
}
#[test]
fn configured_prefix_is_exact_nonempty_case_sensitive_untrimmed_and_not_removed() {
    // Pinned CommitCmdObj uses strings.HasPrefix, NOT word or case-folded matching.
    for (prefix, subject, skip) in [
        ("WIP", "WIP", true),
        ("WIP", "WIPper", true),
        ("WIP", "WIP subject", true),
        ("WIP", "wip subject", false),
        ("WIP", " WIP subject", false),
        ("WIP", "normal subject", false),
        ("", "normal subject", false),
        ("!$", "!$ literal", true),
    ] {
        let hooks = Hooks::new();
        hooks.install(
            "pre-commit",
            "printf ran > \"$(git config core.hooksPath)/pre-ran\"; exit 1",
        );
        hooks.install(
            "commit-msg",
            "printf ran > \"$(git config core.hooksPath)/msg-ran\"; exit 1",
        );
        hooks.install(
            "prepare-commit-msg",
            "printf ran > \"$(git config core.hooksPath)/prepare-ran\"",
        );
        hooks.install(
            "post-commit",
            "printf ran > \"$(git config core.hooksPath)/post-ran\"",
        );
        let mut settings = settings();
        settings.skip_hook_prefix = prefix.into();
        let mut owner = hooks.owner(settings);
        owner.set_draft(subject, "body");
        assert!(owner.submit());
        settle(&mut owner);
        assert_eq!(
            matches!(owner.outcome(), Some(Outcome::Committed { .. })),
            skip,
            "{prefix:?}, {subject:?}"
        );
        assert_eq!(hooks.marker("pre-ran").exists(), !skip);
        assert!(!hooks.marker("msg-ran").exists());
        assert_eq!(hooks.marker("prepare-ran").exists(), skip);
        assert_eq!(hooks.marker("post-ran").exists(), skip);
        if skip {
            let records = hooks.client.history(10).unwrap();
            assert_eq!(records[0].subject, subject.as_bytes());
            assert_eq!(records[0].message, format!("{subject}\n\nbody").as_bytes());
        } else {
            assert!(matches!(
                owner.outcome(),
                Some(Outcome::NotCommitted { .. })
            ));
            assert_eq!(owner.draft().subject, subject);
        }
    }
}
#[test]
fn installed_running_hook_cancellation_settles_then_deliberate_retry_commits_once() {
    let hooks = Hooks::new();
    hooks.install(
        "pre-commit",
        "printf entered > \"$(git config core.hooksPath)/entered\"; sleep 30",
    );
    let mut owner = hooks.owner(settings());
    owner.set_draft("subject", "retained body");
    assert!(owner.submit());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !hooks.marker("entered").exists() {
        assert!(Instant::now() < deadline, "hook did not start");
        std::thread::sleep(Duration::from_millis(2));
    }
    owner.cancel();
    assert!(owner.busy());
    assert!(!owner.submit());
    settle(&mut owner);
    assert!(
        matches!(owner.outcome(), Some(Outcome::NotCommitted { head: git::Head::Unborn { .. }, status }) if status.len() == 1 && status[0].index == b'A')
    );
    assert_eq!(owner.draft().body, "retained body");
    assert!(hooks.client.history(10).unwrap().is_empty());
    hooks.install("pre-commit", "exit 0");
    assert!(owner.submit());
    settle(&mut owner);
    assert!(matches!(owner.outcome(), Some(Outcome::Committed { .. })));
    assert_eq!(hooks.client.history(10).unwrap().len(), 1);
}
#[test]
fn hook_creating_a_commit_then_refusing_outer_command_is_uncertain_not_replayed() {
    let hooks = Hooks::new();
    hooks.install("pre-commit", "git commit --no-verify -qm 'hook side effect' || exit 2\nprintf 'outer hook refused\\n' >&2\nexit 1");
    let mut owner = hooks.owner(settings());
    owner.set_draft("intended subject", "retained body");
    assert!(owner.submit());
    settle(&mut owner);
    assert!(
        matches!(owner.outcome(), Some(Outcome::Uncertain { head: Some(head), status: Some(status) }) if head == &hooks.client.head().unwrap() && status.is_empty())
    );
    assert!(owner.error().unwrap().contains("uncertain"));
    assert_eq!(owner.draft().subject, "intended subject");
    assert_eq!(hooks.client.history(10).unwrap().len(), 1);
    assert_eq!(
        hooks.client.history(10).unwrap()[0].subject,
        b"hook side effect"
    );
}
#[test]
fn failing_post_commit_hook_does_not_override_observed_commit_success() {
    let hooks = Hooks::new();
    hooks.install("post-commit", "printf 'post hook failed\\n' >&2; exit 1");
    let mut owner = hooks.owner(settings());
    owner.set_draft("subject", "body");
    assert!(owner.submit());
    settle(&mut owner);
    assert!(
        matches!(owner.outcome(), Some(Outcome::Committed { head, status }) if head == &hooks.client.head().unwrap() && status.is_empty())
    );
    assert_eq!(hooks.client.history(10).unwrap().len(), 1);
}
