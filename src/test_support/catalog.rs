use crate::connectors::catalog::{Adapter, Client, Record, Reply, Request};
use std::sync::{Arc, Mutex};

pub(crate) fn record(id: &str) -> Record {
    Record {
        id: id.into(),
        title: format!("Record {id}"),
        description: format!("Description {id}"),
    }
}
type Pending = (String, async_channel::Sender<Reply>);

#[derive(Clone, Default)]
pub(crate) struct Controlled {
    pending: Arc<Mutex<Vec<Pending>>>,
}
impl Controlled {
    pub(crate) fn client(&self) -> Client {
        Client::controlled(self.clone())
    }
    pub(crate) fn queries(&self) -> Vec<String> {
        self.pending
            .lock()
            .unwrap()
            .iter()
            .map(|(query, _)| query.clone())
            .collect()
    }
    pub(crate) fn reply(&self, index: usize, reply: Reply) {
        // A canceled request legitimately has a closed receiver.
        let _ = self.pending.lock().unwrap()[index].1.try_send(reply);
    }
}
impl Adapter for Controlled {
    fn list(&self, query: String) -> Request {
        let (send, receive) = async_channel::bounded(1);
        self.pending.lock().unwrap().push((query, send));
        Box::pin(async move { receive.recv().await.expect("test must supply a reply") })
    }
}
