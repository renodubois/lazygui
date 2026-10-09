//! Window-retained read-only HEAD/history owner, independent of file selection.
use crate::git::{Client, Commit, Head, Identity};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub struct Update {
    owner: Arc<()>,
    generation: u64,
    result: Result<(Head, Vec<Commit>), String>,
    _scope: crate::git::WorkflowScope,
}
pub struct History {
    client: Client,
    identity: Identity,
    generation: u64,
    token: Arc<()>,
    head: Option<Head>,
    commits: Vec<Commit>,
    busy: bool,
    error: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
    send: async_channel::Sender<Update>,
    receive: async_channel::Receiver<Update>,
}
impl History {
    pub fn new(client: Client, identity: Identity) -> Self {
        let (send, receive) = async_channel::unbounded();
        let mut owner = Self {
            client,
            identity,
            generation: 0,
            token: Arc::new(()),
            head: None,
            commits: vec![],
            busy: false,
            error: None,
            cancel: None,
            send,
            receive,
        };
        owner.refresh();
        owner
    }
    pub fn identity(&self) -> &Identity {
        &self.identity
    }
    pub fn head(&self) -> Option<&Head> {
        self.head.as_ref()
    }
    pub fn commits(&self) -> &[Commit] {
        &self.commits
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn updates(&self) -> async_channel::Receiver<Update> {
        self.receive.clone()
    }
    pub fn cancel(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Release);
        }
    }
    pub fn refresh(&mut self) {
        if self.busy {
            return;
        }
        self.generation += 1;
        self.busy = true;
        self.error = None;
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        let (client, scope) = match self.client.begin_workflow(cancel) {
            Ok(admitted) => admitted,
            Err(error) => {
                self.busy = false;
                self.cancel = None;
                self.error = Some(error.to_string());
                return;
            }
        };
        let identity = self.identity.clone();
        let generation = self.generation;
        let token = self.token.clone();
        let send = self.send.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("repository-history".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let client = client.bind(&identity)?;
                    Ok((client.head()?, client.history(100)?))
                }))
                .unwrap_or_else(|_| Err(std::io::Error::other("history worker panicked")))
                .map_err(|e: std::io::Error| e.to_string());
                let _ = send.send_blocking(Update {
                    owner: token,
                    generation,
                    result,
                    _scope: scope,
                });
            })
        {
            self.busy = false;
            self.error = Some(error.to_string());
        }
    }
    pub fn apply(&mut self, update: Update) {
        if !Arc::ptr_eq(&self.token, &update.owner) || update.generation != self.generation {
            return;
        }
        self.busy = false;
        self.cancel = None;
        match update.result {
            Ok((head, commits)) => {
                self.head = Some(head);
                self.commits = commits;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
}
impl Drop for History {
    fn drop(&mut self) {
        self.cancel();
        self.receive.close();
        while self.receive.try_recv().is_ok() {}
    }
}
#[cfg(test)]
#[path = "tests/reads.rs"]
mod tests;
