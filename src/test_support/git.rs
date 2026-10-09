use crate::git::{
    Client,
    process::{Executor, Native, Output},
};
use std::{
    io,
    process::Command,
    sync::{Arc, atomic::AtomicBool},
};
pub(crate) struct Fixture {
    pub root: tempfile::TempDir,
}
struct Isolated {
    home: std::path::PathBuf,
}
impl Executor for Isolated {
    fn execute(
        &self,
        mut command: Command,
        input: Vec<u8>,
        cancel: Arc<AtomicBool>,
    ) -> io::Result<Output> {
        let args: Vec<_> = command.get_args().map(|x| x.to_owned()).collect();
        let cwd = command.get_current_dir().unwrap().to_owned();
        let mut clean = Command::new("/usr/bin/git");
        clean
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgSign=false",
                "-c",
                "tag.gpgSign=false",
                "-c",
                "credential.helper=",
            ])
            .args(args)
            .current_dir(cwd);
        if let Some((_, Some(value))) = command
            .get_envs()
            .find(|(key, _)| *key == "GIT_OPTIONAL_LOCKS")
        {
            clean.env("GIT_OPTIONAL_LOCKS", value);
        }
        // Avoid inherited repository, helper, signing, and template settings.
        command.env_clear();
        Native.execute(clean, input, cancel)
    }
}
impl Fixture {
    pub fn new() -> Self {
        let fixture = Self {
            root: tempfile::tempdir().unwrap(),
        };
        fixture.run(&["init", "-q"]);
        fixture
    }
    pub fn client_at(&self, path: std::path::PathBuf) -> Client {
        Client::with_executor(
            path,
            Arc::new(Isolated {
                home: self.root.path().join("home"),
            }),
        )
    }
    pub fn client(&self) -> Client {
        self.client_at(self.root.path().to_owned())
    }
    pub fn run(&self, args: &[&str]) -> Vec<u8> {
        let mut command = Command::new("git");
        command.current_dir(self.root.path()).args(args);
        let output = Isolated {
            home: self.root.path().join("home"),
        }
        .execute(command, vec![], Arc::new(AtomicBool::new(false)))
        .unwrap();
        assert_eq!(output.code, Some(0), "fixture command failed: {args:?}");
        output.stdout
    }
    pub fn write(&self, path: &str, bytes: &[u8]) {
        std::fs::write(self.root.path().join(path), bytes).unwrap();
    }
    pub fn commit(&self) {
        self.run(&["add", "."]);
        self.run(&["commit", "-qm", "fixture"]);
    }
}
