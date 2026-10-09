//! Typed catalog capability. Concrete adapters do not leak transport to callers.
mod error;
mod http;
mod memory;
mod types;

pub(crate) use error::Error;
use std::{future::Future, pin::Pin, sync::Arc};
pub(crate) use types::Record;

pub(crate) type Reply = Result<Vec<Record>, Error>;
pub(crate) type Request = Pin<Box<dyn Future<Output = Reply> + Send>>;

pub(crate) trait Adapter: Send + Sync {
    fn list(&self, query: String) -> Request;
}
#[derive(Clone)]
pub(crate) struct Client(Arc<dyn Adapter>);
impl Client {
    pub(crate) fn memory() -> Self {
        Self(Arc::new(memory::Memory))
    }
    pub(crate) fn http(url: &str) -> Result<Self, Error> {
        Ok(Self(Arc::new(http::Http::new(url)?)))
    }
    pub(crate) fn list(&self, query: String) -> Request {
        self.0.list(query)
    }
    #[cfg(test)]
    pub(crate) fn controlled(adapter: impl Adapter + 'static) -> Self {
        Self(Arc::new(adapter))
    }
}
#[cfg(test)]
#[path = "tests/binding.rs"]
mod tests;
