use gpui_kit::{App, Entity, TestAppContext, VisualTestContext, component::Root};
use lazygui::{
    git::{
        MutationGates,
        process::{Executor, Native, Output, ProcessHost},
    },
    lazygit_config::{DiscoveryOptions, gui::OrderedStorage},
    repository::Repository,
};
use std::{
    io,
    process::Command,
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct State {
    pub staged: bool,
    pub committed: bool,
    pub nested: bool,
    pub mixed: bool,
    pub writes: usize,
    pub patches: Vec<Vec<u8>>,
}
pub(crate) struct Git {
    pub state: Mutex<State>,
}
impl Executor for Git {
    fn execute(&self, command: Command, input: Vec<u8>, _: Arc<AtomicBool>) -> io::Result<Output> {
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let root = command.get_current_dir().unwrap();
        let mut state = self.state.lock().unwrap();
        let has = |key: &str| args.iter().any(|a| a == key);
        let stdout = if has("--version") {
            b"git version 2.56.0\n".to_vec()
        } else if has("--is-bare-repository") {
            b"false\n".to_vec()
        } else if has("--show-toplevel") {
            format!("{}\n", root.display()).into_bytes()
        } else if has("--absolute-git-dir") || has("--git-common-dir") {
            format!("{}/.git\n", root.display()).into_bytes()
        } else if has("symbolic-ref") {
            b"refs/heads/main\n".to_vec()
        } else if has("HEAD^{commit}") {
            format!("{}\n", if state.committed { "b" } else { "a" }.repeat(40)).into_bytes()
        } else if has("status") {
            if state.committed {
                vec![]
            } else {
                let prefix = if state.nested { "dir/" } else { "" };
                let status = if state.mixed {
                    "MM"
                } else if state.staged {
                    "M "
                } else {
                    " M"
                };
                format!("{status} {prefix}a.txt\0 M {prefix}b.txt\0").into_bytes()
            }
        } else if has("diff") {
            let path = args.last().unwrap();
            let available = !state.committed
                && ((path.ends_with("a.txt") && (state.mixed || has("--cached") == state.staged))
                    || (path.ends_with("b.txt") && !has("--cached")));
            if available {
                format!("diff --git a/{path} b/{path}\nindex 1111111..2222222 100644\n--- a/{path}\n+++ b/{path}\n@@ -1,3 +1,3 @@\n one\n-old\n+new\n three\n").into_bytes()
            } else {
                vec![]
            }
        } else if has("log") {
            format!(
                "{}\0\0Fixture\01700000000\0{}\0",
                if state.committed { "b" } else { "a" }.repeat(40),
                if state.committed {
                    "connected commit"
                } else {
                    "fixture"
                }
            )
            .into_bytes()
        } else if has("cat-file") && has("--batch") {
            String::from_utf8(input).unwrap().lines().flat_map(|oid| {
                let subject = if state.committed { "connected commit" } else { "fixture" };
                let object = format!("tree {}\nauthor Fixture <fixture@example.invalid> 1700000000 +0000\ncommitter Fixture <fixture@example.invalid> 1700000000 +0000\n\n{subject}\n\nbody hard break\nsecond line\n\nSigned-off-by: Fixture <fixture@example.invalid>\n", "c".repeat(40));
                format!("{oid} commit {}\n{object}\n", object.len()).into_bytes()
            }).collect()
        } else if has("add") || has("apply") {
            if has("apply") {
                state.patches.push(input);
            }
            state.staged = true;
            state.mixed = false;
            state.writes += 1;
            vec![]
        } else if has("reset") {
            state.staged = false;
            state.writes += 1;
            vec![]
        } else if has("commit") {
            state.committed = true;
            state.writes += 1;
            vec![]
        } else {
            return Err(io::Error::other(format!(
                "unexpected fixture request {args:?}"
            )));
        };
        Ok(Output {
            code: Some(0),
            stdout,
            stderr: vec![],
            cancelled: false,
            truncated: false,
        })
    }
}
/// Installed Git in disposable storage, never inheriting user helpers/config/hooks.
pub(crate) struct Isolated {
    home: std::path::PathBuf,
    hooks: std::path::PathBuf,
    pub commands: Mutex<Vec<Vec<std::ffi::OsString>>>,
}
impl Executor for Isolated {
    fn execute(
        &self,
        command: Command,
        input: Vec<u8>,
        cancel: Arc<AtomicBool>,
    ) -> io::Result<Output> {
        self.commands
            .lock()
            .unwrap()
            .push(command.get_args().map(std::ffi::OsString::from).collect());
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
            .arg("-c")
            .arg(format!("core.hooksPath={}", self.hooks.display()))
            .args([
                "-c",
                "commit.gpgSign=false",
                "-c",
                "tag.gpgSign=false",
                "-c",
                "credential.helper=",
            ])
            .args(command.get_args())
            .current_dir(command.get_current_dir().unwrap());
        if let Some((_, Some(value))) = command
            .get_envs()
            .find(|(key, _)| *key == "GIT_OPTIONAL_LOCKS")
        {
            clean.env("GIT_OPTIONAL_LOCKS", value);
        }
        Native.execute(clean, input, cancel)
    }
}
/// Hold one real transport request while the production owner retains its workflow.
pub(crate) struct HeldRequest {
    pub armed: Arc<AtomicBool>,
    pub started: std::sync::mpsc::Receiver<()>,
    pub release: std::sync::mpsc::Sender<()>,
    pub cancellation: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}
struct HoldTransport {
    inner: Arc<dyn Executor>,
    verb: &'static str,
    armed: Arc<AtomicBool>,
    started: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
    cancellation: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}
impl Executor for HoldTransport {
    fn execute(
        &self,
        command: Command,
        input: Vec<u8>,
        cancel: Arc<AtomicBool>,
    ) -> io::Result<Output> {
        if command.get_args().any(|arg| arg == self.verb)
            && self.armed.swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            *self.cancellation.lock().unwrap() = Some(cancel.clone());
            self.started.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
        }
        self.inner.execute(command, input, cancel)
    }
}
pub(crate) struct Fixture {
    pub temp: tempfile::TempDir,
    pub git: Arc<Git>,
    pub host: ProcessHost,
    pub options: DiscoveryOptions,
    pub storage: OrderedStorage,
    pub executor: Option<Arc<dyn Executor>>,
    pub installed_git: Option<Arc<Isolated>>,
}
impl Fixture {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        let config = temp.path().join("config.yml");
        std::fs::write(
            &config,
            "git:\n  autoRefresh: false\n  autoDetectExternalChanges: false\n",
        )
        .unwrap();
        let options = DiscoveryOptions {
            cwd: root,
            home: Some(temp.path().into()),
            cli_config_file: Some(config.as_os_str().into()),
            xdg_config_dirs: vec![],
            ..Default::default()
        };
        let (storage, _, _) = OrderedStorage::open(temp.path().join("profile")).unwrap();
        Self {
            temp,
            git: Arc::new(Git {
                state: Mutex::new(State::default()),
            }),
            host: ProcessHost::new(),
            options,
            storage,
            executor: None,
            installed_git: None,
        }
    }
    /// Empty isolated repository; hooks are enabled only inside this fixture.
    pub fn installed_empty() -> Self {
        let mut fixture = Self::new();
        let home = fixture.temp.path().join("home");
        let hooks = fixture.temp.path().join("hooks");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&hooks).unwrap();
        let executor = Arc::new(Isolated {
            home,
            hooks,
            commands: Mutex::new(vec![]),
        });
        fixture.executor = Some(executor.clone());
        fixture.installed_git = Some(executor);
        fixture.run_git(&["init", "-q", "-b", "main"]);
        fixture
    }
    pub fn installed() -> Self {
        let fixture = Self::installed_empty();
        fixture.write("a.txt", b"one\nold\nthree\n");
        fixture.write("b.txt", b"one\nold\nthree\n");
        fixture.run_git(&["add", "."]);
        fixture.run_git(&["commit", "-qm", "fixture\n\nbody hard break\nsecond line\n\nSigned-off-by: Fixture <fixture@example.invalid>"]);
        fixture.write("a.txt", b"one\nnew\nthree\n");
        fixture.write("b.txt", b"one\nnew\nthree\n");
        fixture
    }
    pub fn hook(&self, name: &str, script: &str) {
        use std::os::unix::fs::PermissionsExt;
        let path = self.temp.path().join("hooks").join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    pub fn config(&self, yaml: &str) {
        std::fs::write(self.temp.path().join("config.yml"), yaml).unwrap();
    }
    pub fn write(&self, path: impl AsRef<std::path::Path>, bytes: &[u8]) {
        let path = self.options.cwd.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    pub fn run_git(&self, args: &[&str]) -> Vec<u8> {
        self.run_git_os(&args.iter().map(std::ffi::OsStr::new).collect::<Vec<_>>())
    }
    pub fn run_git_os(&self, args: &[&std::ffi::OsStr]) -> Vec<u8> {
        let mut command = Command::new("git");
        command.current_dir(&self.options.cwd).args(args);
        let result = self
            .executor
            .as_ref()
            .unwrap()
            .execute(command, vec![], Arc::new(AtomicBool::new(false)))
            .unwrap();
        assert_eq!(
            result.code,
            Some(0),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        result.stdout
    }
    pub fn hold(&mut self, verb: &'static str) -> HeldRequest {
        let (started, receive) = std::sync::mpsc::channel();
        let (release, held) = std::sync::mpsc::channel();
        let armed = Arc::new(AtomicBool::new(false));
        let cancellation = Arc::new(Mutex::new(None));
        self.executor = Some(Arc::new(HoldTransport {
            inner: self.executor.clone().unwrap(),
            verb,
            armed: armed.clone(),
            started,
            release: Mutex::new(held),
            cancellation: cancellation.clone(),
        }));
        HeldRequest {
            armed,
            started: receive,
            release,
            cancellation,
        }
    }
    pub fn repository(&self) -> Repository {
        Repository::with_executor(
            self.host.clone(),
            self.options.clone(),
            self.executor.clone().unwrap_or_else(|| self.git.clone()),
        )
    }
}
pub(crate) fn open<'a>(
    cx: &'a mut TestAppContext,
    fixture: &Fixture,
) -> (
    &'a mut VisualTestContext,
    Entity<crate::views::app_shell::AppShell>,
) {
    cx.update(gpui_kit::init);
    let repository = fixture.repository();
    let host = fixture.host.clone();
    let storage = fixture.storage.clone();
    let mut shell = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        let root = crate::views::app_shell::open(
            window,
            cx,
            host,
            Arc::new(MutationGates::new()),
            storage,
            repository,
        );
        shell = Some(root.clone());
        Root::new(root, window, cx)
    });
    (visual, shell.unwrap())
}
pub(crate) fn screen(
    shell: &crate::views::app_shell::AppShell,
) -> Entity<crate::views::repository::RepositoryView> {
    shell.screen.clone()
}
/// Shutdown can retain a workflow inside an opaque owner update. Never block the
/// GUI thread: keep the production delivery tasks running until acknowledgment.
pub(crate) fn shutdown(visual: &mut VisualTestContext, host: &ProcessHost) {
    let acknowledgment = host.shutdown();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !acknowledgment.is_complete() {
        visual
            .background_executor
            .advance_clock(Duration::from_millis(16));
        visual.run_until_parked();
        assert!(
            Instant::now() < deadline,
            "process shutdown did not acknowledge after GUI delivery"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(acknowledgment.is_complete());
}

pub(crate) fn wait(visual: &mut VisualTestContext, condition: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        visual.run_until_parked();
        if visual.update(|_, cx| condition(cx)) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "worker completion did not reach the retained owner"
        );
        // Let the installed-Git workers make progress before moving fake GUI time.
        // An unthrottled spin can advance many polling intervals while one child
        // is still starting, starving assertions of a settled owner snapshot.
        std::thread::sleep(Duration::from_millis(1));
        visual
            .background_executor
            .advance_clock(Duration::from_millis(16));
    }
}
