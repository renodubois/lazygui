//! Window-local repository transition contract. No tabs or global repository state.
use crate::{
    git::{
        Client, Identity, Readiness,
        process::{Executor, Native, ProcessHost},
    },
    lazygit_config::{DiscoveryOptions, Settings, SourceReport},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// A successful open has one immutable identity and a client bound to that identity.
/// Settings are discovered only after repository discovery, on the same worker.
pub struct Session {
    pub client: Client,
    pub readiness: Readiness,
    pub settings: Settings,
    /// Successful discovery feedback, without retaining another copy of source YAML.
    pub source_reports: Vec<SourceReport>,
}
pub struct Update {
    owner: Arc<()>,
    generation: u64,
    result: Result<Session, String>,
    reload: bool,
    _scope: crate::git::WorkflowScope,
}
/// Window-local readiness/configuration owner. A switch invalidates old results
/// immediately; cancellation is a resource policy, generation is the correctness gate.
pub struct Repository {
    host: ProcessHost,
    executor: Arc<dyn Executor>,
    options: DiscoveryOptions,
    generation: u64,
    token: Arc<()>,
    session: Option<Session>,
    loading: bool,
    error: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
    send: async_channel::Sender<Update>,
    receive: async_channel::Receiver<Update>,
}
impl Repository {
    pub fn open(host: ProcessHost, options: DiscoveryOptions) -> Self {
        Self::with_executor(host, options, Arc::new(Native))
    }
    /// Focused Git executor substitution; uses the identical readiness/config workflow.
    pub fn with_executor(
        host: ProcessHost,
        mut options: DiscoveryOptions,
        executor: Arc<dyn Executor>,
    ) -> Self {
        options.anchor_global_sources();
        let (send, receive) = async_channel::unbounded();
        let path = options.cwd.clone();
        let mut owner = Self {
            host,
            executor,
            options,
            generation: 0,
            token: Arc::new(()),
            session: None,
            loading: false,
            error: None,
            cancel: None,
            send,
            receive,
        };
        owner.switch_in_place(path);
        owner
    }
    pub fn updates(&self) -> async_channel::Receiver<Update> {
        self.receive.clone()
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }
    pub fn loading(&self) -> bool {
        self.loading
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn cancel(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Release);
        }
    }
    pub fn switch_in_place(&mut self, path: PathBuf) {
        self.cancel();
        self.generation += 1;
        self.session = None;
        self.options.cwd = if path.is_absolute() {
            path
        } else {
            self.options.cwd.join(path)
        };
        self.options.repository_root = None;
        self.options.git_dir = None;
        self.start(false);
    }
    /// Transactional reload: a failure retains the active session/settings.
    pub fn reload_config(&mut self) {
        if self.loading || self.session.is_none() {
            return;
        }
        self.cancel();
        self.generation += 1;
        self.start(true);
    }
    fn start(&mut self, reload: bool) {
        self.loading = true;
        self.error = None;
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        let mut options = self.options.clone();
        // Readiness/client binding is fixed for the open identity. Reload only
        // validates a new config transaction; it does not silently rediscover Git.
        let existing = if reload {
            self.session
                .as_ref()
                .map(|s| (s.client.clone(), s.readiness.clone()))
        } else {
            None
        };
        let base =
            Client::with_executor(options.cwd.clone(), self.host.retain(self.executor.clone()));
        let (client, scope) = match base.begin_workflow(cancel) {
            Ok(admitted) => admitted,
            Err(error) => {
                self.loading = false;
                self.cancel = None;
                self.error = Some(error.to_string());
                return;
            }
        };
        let send = self.send.clone();
        let generation = self.generation;
        let token = self.token.clone();
        let spawned = std::thread::Builder::new()
            .name("repository-open".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let (client, readiness) = if let Some(existing) = existing {
                        existing
                    } else {
                        let readiness = client
                            .readiness()
                            .map_err(|e| format!("Cannot open repository: {e}"))?;
                        let client = client
                            .bind(&readiness.identity)
                            .map_err(|e| e.to_string())?;
                        (client, readiness)
                    };
                    options.repository_root = readiness.identity.worktree.clone();
                    options.git_dir = Some(readiness.identity.git_dir.clone());
                    let (settings, discovery) = Settings::load_discovered(&options)
                        .map_err(|e| format!("Configuration: {e}"))?;
                    Ok(Session {
                        client: client.without_workflow(),
                        readiness,
                        settings,
                        source_reports: discovery.reports,
                    })
                }))
                .unwrap_or_else(|_| Err("repository worker panicked".into()));
                let _ = send.send_blocking(Update {
                    owner: token,
                    generation,
                    result,
                    reload,
                    _scope: scope,
                });
            });
        if let Err(error) = spawned {
            self.loading = false;
            self.error = Some(error.to_string());
        }
    }
    /// Returns true only when a current session/settings replacement was accepted.
    pub fn apply(&mut self, update: Update) -> bool {
        if !Arc::ptr_eq(&self.token, &update.owner) || update.generation != self.generation {
            return false;
        }
        self.loading = false;
        self.cancel = None;
        match update.result {
            Ok(session) => {
                self.session = Some(session);
                self.error = None;
                true
            }
            Err(error) => {
                if !update.reload {
                    self.session = None;
                }
                self.error = Some(error);
                false
            }
        }
    }
}
impl Drop for Repository {
    fn drop(&mut self) {
        self.cancel();
        self.receive.close();
        while self.receive.try_recv().is_ok() {}
    }
}
pub struct Navigation {
    current: Identity,
    parents: Vec<Identity>,
    generation: u64,
}
impl Navigation {
    pub fn new(current: Identity) -> Self {
        Self {
            current,
            parents: vec![],
            generation: 1,
        }
    }
    pub fn current(&self) -> &Identity {
        &self.current
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn switch_in_place(&mut self, next: Identity) {
        self.current = next;
        self.parents.clear();
        self.generation += 1;
    }
    pub fn enter_submodule(&mut self, next: Identity) {
        self.parents
            .push(std::mem::replace(&mut self.current, next));
        self.generation += 1;
    }
    pub fn return_parent(&mut self) -> bool {
        if let Some(parent) = self.parents.pop() {
            self.current = parent;
            self.generation += 1;
            true
        } else {
            false
        }
    }
    pub fn accepts(&self, generation: u64) -> bool {
        generation == self.generation
    }
}
#[cfg(test)]
#[path = "tests/readiness.rs"]
mod readiness_tests;
#[cfg(test)]
#[path = "tests/navigation.rs"]
mod tests;
