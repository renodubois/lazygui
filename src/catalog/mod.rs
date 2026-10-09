//! Non-rendering feature owner. The shell delivers opaque updates once.
mod state;
use crate::{
    connectors::catalog::{Client, Reply},
    runtime::{Execution, Work},
    storage::{Config, Persistence},
};
pub(crate) use state::State;
use std::time::Duration;

pub(crate) struct Update(UpdateKind);
enum UpdateKind {
    Loaded(u64, Reply),
    Saved(u64, Result<(), String>),
}
pub(crate) struct Catalog {
    state: State,
    client: Client,
    execution: Execution,
    persistence: Option<Persistence>,
    save_revision: u64,
    storage_warning: Option<String>,
    updates: async_channel::Receiver<Update>,
    deliver: async_channel::Sender<Update>,
    load_task: Option<Work>,
    save_tasks: Vec<Work>,
}
impl Catalog {
    pub(crate) fn new(
        client: Client,
        execution: Execution,
        persistence: Option<Persistence>,
        warning: Option<String>,
    ) -> Self {
        let (deliver, updates) = async_channel::bounded(16);
        Self {
            state: State::default(),
            client,
            execution,
            persistence,
            storage_warning: warning,
            save_revision: 0,
            updates,
            deliver,
            load_task: None,
            save_tasks: Vec::new(),
        }
    }
    pub(crate) fn read(&self) -> &State {
        &self.state
    }
    pub(crate) fn storage_warning(&self) -> Option<&str> {
        self.storage_warning.as_deref()
    }
    // A result receiver is not a broadcast subscription; only the stable host consumes it.
    pub(crate) fn updates(&self) -> async_channel::Receiver<Update> {
        self.updates.clone()
    }
    pub(crate) fn load(&mut self, query: String) {
        let generation = self.state.begin(query.clone());
        let request = self.client.list(query);
        let deadline = self.execution.sleep(Duration::from_secs(6));
        let deliver = self.deliver.clone();
        self.load_task = Some(self.execution.start(async move {
            let result = tokio::select! {
                biased;
                _ = deadline => Err(crate::connectors::catalog::Error::Timeout),
                result = request => result,
            };
            let _ = deliver
                .send(Update(UpdateKind::Loaded(generation, result)))
                .await;
        }));
    }
    pub(crate) fn search(&mut self, query: String) {
        let query = query.trim().to_string();
        self.load(query.clone());
        if let Some(persistence) = &self.persistence {
            self.save_revision += 1;
            let revision = self.save_revision;
            let reply = persistence.save(Config { query });
            let deliver = self.deliver.clone();
            // Superseded waiters may be canceled; the ordered worker still completes writes.
            self.save_tasks.clear();
            self.save_tasks.push(self.execution.start(async move {
                let result = reply
                    .recv()
                    .await
                    .unwrap_or_else(|_| Err("Preference worker stopped.".into()));
                let _ = deliver
                    .send(Update(UpdateKind::Saved(revision, result)))
                    .await;
            }));
        }
    }
    pub(crate) fn select(&mut self, id: &str) {
        self.state.select(id);
    }
    pub(crate) fn apply(&mut self, update: Update) {
        match update.0 {
            UpdateKind::Loaded(generation, result) => self.state.complete(generation, result),
            UpdateKind::Saved(revision, result) if revision == self.save_revision => {
                self.storage_warning = result.err();
                self.save_tasks.clear();
            }
            UpdateKind::Saved(..) => {}
        }
    }
}
#[cfg(test)]
#[path = "tests/coordinator.rs"]
mod tests;
