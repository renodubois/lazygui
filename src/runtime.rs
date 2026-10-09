//! Executor substitution only. Callers own tasks, deadlines and result policy.
use gpui_kit::BackgroundExecutor;
use std::{future::Future, sync::OnceLock, time::Duration};

#[derive(Clone)]
pub(crate) struct Execution {
    executor: BackgroundExecutor,
    #[cfg(test)]
    controlled: bool,
}

pub(crate) enum Work {
    Tokio(tokio::task::JoinHandle<()>),
    #[cfg(test)]
    Controlled {
        _task: gpui_kit::Task<()>,
    },
}
impl Drop for Work {
    fn drop(&mut self) {
        match self {
            Self::Tokio(task) => task.abort(),
            #[cfg(test)]
            Self::Controlled { .. } => {}
        }
        // GPUI Task is cancel-on-drop.
    }
}
impl Execution {
    pub(crate) fn production(executor: BackgroundExecutor) -> Self {
        Self {
            executor,
            #[cfg(test)]
            controlled: false,
        }
    }
    #[cfg(test)]
    pub(crate) fn controlled(executor: BackgroundExecutor) -> Self {
        Self {
            executor,
            controlled: true,
        }
    }
    pub(crate) fn sleep(&self, duration: Duration) -> gpui_kit::Task<()> {
        self.executor.timer(duration)
    }
    pub(crate) fn start(&self, future: impl Future<Output = ()> + Send + 'static) -> Work {
        #[cfg(test)]
        if self.controlled {
            return Work::Controlled {
                _task: self.executor.spawn(future),
            };
        }
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        Work::Tokio(
            RUNTIME
                .get_or_init(|| tokio::runtime::Runtime::new().expect("I/O runtime"))
                .spawn(future),
        )
    }
}
