//! Stable window host: retains independent owners and their single opaque update consumers.
use super::repository::RepositoryView;
use gpui_kit::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Task, Window, div,
};
use lazygui::{
    commit::Commit,
    git::{DiffOptions, MutationGates, Whitespace, process::ProcessHost},
    history::History,
    lazygit_config::gui::OrderedStorage,
    repository::Repository,
    working_tree::WorkingTree,
};
use std::{path::PathBuf, sync::Arc, time::Duration};

pub(crate) fn open(
    window: &mut Window,
    cx: &mut App,
    host: ProcessHost,
    gates: Arc<MutationGates>,
    storage: OrderedStorage,
    repository: Repository,
) -> Entity<AppShell> {
    let shell = cx.new(|cx| AppShell::new(window, cx, host, gates, storage, repository));
    let weak = shell.downgrade();
    window.on_window_should_close(cx, move |window, cx| {
        let _ = weak.update(cx, |shell, cx| shell.request_close(false, window, cx));
        false
    });
    shell
}
struct Features {
    tree: Entity<WorkingTree>,
    history: Entity<History>,
    commit: Entity<Commit>,
    _deliveries: Vec<Task<()>>,
    _refresh: Task<()>,
}
pub(crate) struct AppShell {
    repository: Entity<Repository>,
    screen: Entity<RepositoryView>,
    host: ProcessHost,
    gates: Arc<MutationGates>,
    storage: OrderedStorage,
    features: Option<Features>,
    _repository_delivery: Task<()>,
    closing: Option<Task<()>>,
}
impl AppShell {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        host: ProcessHost,
        gates: Arc<MutationGates>,
        storage: OrderedStorage,
        owner: Repository,
    ) -> Self {
        let repository = cx.new(|_| owner);
        let weak = cx.entity().downgrade();
        let screen = cx.new(|cx| {
            RepositoryView::new(repository.clone(), window, cx, move |intent, window, cx| {
                let weak = weak.clone();
                // Intent callbacks run inside the screen's update. Defer replacement
                // until that borrow is released, rather than reentering the screen.
                window.defer(cx, move |window, cx| {
                    let _ = weak.update(cx, |shell, cx| match intent {
                        super::repository::HostIntent::Switch(path) => {
                            shell.switch(path, window, cx)
                        }
                        super::repository::HostIntent::Close => {
                            shell.request_close(false, window, cx)
                        }
                        super::repository::HostIntent::CloseConfirmed => {
                            shell.request_close(true, window, cx)
                        }
                    });
                });
            })
        });
        let updates = repository.read(cx).updates();
        let delivery = cx.spawn(async move |weak, cx| {
            while let Ok(update) = receive_update(&updates, cx.background_executor()).await {
                if weak
                    .update(cx, |shell, cx| {
                        let accepted = shell.repository.update(cx, |owner, cx| {
                            let accepted = owner.apply(update);
                            cx.notify();
                            accepted
                        });
                        if accepted && shell.closing.is_none() {
                            shell.install_features(cx);
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            repository,
            screen,
            host,
            gates,
            storage,
            features: None,
            _repository_delivery: delivery,
            closing: None,
        }
    }
    fn switch(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing.is_some() {
            return;
        }
        if self.features.as_ref().is_some_and(|features| {
            features.tree.read(cx).busy() || features.commit.read(cx).busy()
        }) {
            self.screen.update(cx, |view, cx| view.refuse_switch(cx));
            return;
        }
        self.screen
            .update(cx, |view, cx| view.clear_features(window, cx));
        self.features = None;
        self.repository.update(cx, |owner, cx| {
            owner.switch_in_place(path);
            cx.notify();
        });
        cx.notify();
    }
    fn install_features(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.repository.read(cx).session() else {
            return;
        };
        let client = session.client.clone();
        let identity = session.readiness.identity.clone();
        let settings = session.settings.m1().clone();
        // A validated config reload replaces policies together, retaining the draft.
        // The view refuses reload while a mutation is pending.
        let retained_draft = self
            .features
            .as_ref()
            .filter(|features| features.tree.read(cx).identity() == &identity)
            .map(|features| features.commit.read(cx).draft().clone());
        let tree = cx.new(|_| {
            let mut owner = WorkingTree::new(
                client.clone(),
                identity.clone(),
                self.gates.clone(),
                diff_options(&settings),
            );
            let mode = if settings.diff.use_hunk_mode {
                lazygui::working_tree::SelectionMode::Hunk
            } else {
                lazygui::working_tree::SelectionMode::Line
            };
            owner.set_selection_mode(lazygui::git::Side::Worktree, mode);
            owner.set_selection_mode(lazygui::git::Side::Index, mode);
            owner
        });
        let history = cx.new(|_| History::new(client.clone(), identity.clone()));
        let commit = cx.new(|_| {
            let mut owner = Commit::new(client, identity, self.gates.clone(), settings.clone());
            if let Some(draft) = retained_draft {
                owner.set_draft(draft.subject, draft.body);
            }
            owner
        });
        let updates = tree.read(cx).updates();
        let target = tree.downgrade();
        let tree_delivery = cx.spawn(async move |_, cx| {
            while let Ok(update) = receive_update(&updates, cx.background_executor()).await {
                if target
                    .update(cx, |owner, cx| {
                        owner.apply(update);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let updates = history.read(cx).updates();
        let target = history.downgrade();
        let history_delivery = cx.spawn(async move |_, cx| {
            while let Ok(update) = receive_update(&updates, cx.background_executor()).await {
                if target
                    .update(cx, |owner, cx| {
                        owner.apply(update);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let updates = commit.read(cx).updates();
        let target = commit.downgrade();
        let tree_target = tree.downgrade();
        let history_target = history.downgrade();
        let commit_delivery = cx.spawn(async move |_, cx| {
            while let Ok(update) = receive_update(&updates, cx.background_executor()).await {
                let accepted = target.update(cx, |owner, cx| {
                    let accepted = owner.apply(update);
                    cx.notify();
                    accepted
                });
                match accepted {
                    Err(_) => break,
                    Ok(false) => continue,
                    Ok(true) => {}
                }
                let _ = tree_target.update(cx, |owner, cx| {
                    owner.refresh();
                    cx.notify();
                });
                let _ = history_target.update(cx, |owner, cx| {
                    owner.refresh();
                    cx.notify();
                });
            }
        });
        let tree_target = tree.downgrade();
        let history_target = history.downgrade();
        let policy = settings.refresh.clone();
        let refresh = cx.spawn(async move |_, cx| {
            if !policy.auto_refresh && !policy.auto_detect_external_changes {
                return;
            }
            let interval = if policy.auto_detect_external_changes {
                policy.external_change_check_interval_seconds
            } else {
                policy.refresh_interval_seconds
            }
            .max(1);
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(interval))
                    .await;
                if tree_target
                    .update(cx, |owner, cx| {
                        owner.auto_refresh();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                let _ = history_target.update(cx, |owner, cx| {
                    if !owner.busy() {
                        owner.refresh();
                    }
                    cx.notify();
                });
            }
        });
        self.screen.update(cx, |view, cx| {
            view.set_features(tree.clone(), history.clone(), commit.clone(), settings, cx)
        });
        self.features = Some(Features {
            tree,
            history,
            commit,
            _deliveries: vec![tree_delivery, history_delivery, commit_delivery],
            _refresh: refresh,
        });
    }
    fn request_close(&mut self, confirmed: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing.is_some() {
            return;
        }
        if !confirmed
            && self
                .screen
                .update(cx, |view, cx| view.confirm_close(window, cx))
        {
            return;
        }
        self.repository.update(cx, |owner, _| owner.cancel());
        if let Some(features) = &self.features {
            features.history.update(cx, |owner, _| owner.cancel());
            features.commit.update(cx, |owner, _| owner.cancel());
        }
        self.screen
            .update(cx, |view, cx| view.clear_features(window, cx));
        // Dropping retained owners requests cancellation; host retains settlement.
        self.features = None;
        let host = self.host.clone();
        // A sole window cancels immediately. Multiple simultaneous closes must
        // re-evaluate inside the serialized removal update, after their flush.
        let acknowledgment = (cx.windows().len() == 1).then(|| host.shutdown());
        let flush = self.storage.flush();
        self.closing = Some(cx.spawn_in(window, async move |_, cx| {
            if let Some(acknowledgment) = acknowledgment {
                while !acknowledgment.is_complete() {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                }
            }
            let _ = receive_update(&flush, cx.background_executor()).await;
            let last = cx
                .update(|window, cx| {
                    if cx.windows().len() > 1 {
                        window.remove_window();
                        false
                    } else {
                        true
                    }
                })
                .unwrap_or(false);
            if last {
                let acknowledgment = host.shutdown();
                while !acknowledgment.is_complete() {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                }
                let _ = cx.update(|window, cx| {
                    window.remove_window();
                    if cx.windows().is_empty() {
                        cx.quit();
                    }
                });
            }
        }));
        cx.notify();
    }
}
// A bounded GUI timer bridges native worker channels without registering GPUI
// wakers on foreign threads. The same delivery works with GPUI's fake clock.
async fn receive_update<T>(
    updates: &async_channel::Receiver<T>,
    executor: &gpui_kit::BackgroundExecutor,
) -> Result<T, async_channel::RecvError> {
    loop {
        match updates.try_recv() {
            Ok(update) => return Ok(update),
            Err(async_channel::TryRecvError::Closed) => return Err(async_channel::RecvError),
            Err(async_channel::TryRecvError::Empty) => {
                executor.timer(Duration::from_millis(16)).await
            }
        }
    }
}
fn diff_options(settings: &lazygui::lazygit_config::M1Settings) -> DiffOptions {
    DiffOptions {
        context: settings.diff.context_size.min(u32::MAX as usize) as u32,
        whitespace: if settings.diff.ignore_whitespace {
            Whitespace::IgnoreAllSpace
        } else {
            Whitespace::Exact
        },
    }
}
impl Render for AppShell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.screen.clone())
    }
}
#[cfg(test)]
#[path = "tests/close.rs"]
mod close_tests;
#[cfg(test)]
#[path = "tests/composition.rs"]
mod composition_tests;
#[cfg(test)]
#[path = "tests/configured.rs"]
mod configured_tests;
#[cfg(test)]
#[path = "tests/installed.rs"]
mod installed_tests;
#[cfg(test)]
#[path = "tests/partial.rs"]
mod partial_tests;
#[cfg(test)]
#[path = "tests/presentation.rs"]
mod presentation_tests;
#[cfg(test)]
#[path = "tests/support/mod.rs"]
pub(super) mod test_support;
#[cfg(test)]
#[path = "tests/shell.rs"]
mod tests;
