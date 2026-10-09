use super::{Adapter, Record, Request};

pub(super) struct Memory;
impl Adapter for Memory {
    fn list(&self, query: String) -> Request {
        Box::pin(async move {
            let records = [
                (
                    "views",
                    "Views",
                    "Rendering, focus, input controls and private child composition.",
                ),
                (
                    "features",
                    "Feature owners",
                    "Longer-lived state, intentions, requests and stale-result rejection.",
                ),
                (
                    "connectors",
                    "Connectors",
                    "Typed external operations with private transport and wire decoding.",
                ),
                (
                    "storage",
                    "Storage",
                    "Nonsensitive preferences and explicit file/provider mechanics.",
                ),
            ];
            let query = query.to_lowercase();
            Ok(records
                .into_iter()
                .filter(|(_, title, description)| {
                    format!("{title} {description}")
                        .to_lowercase()
                        .contains(&query)
                })
                .map(|(id, title, description)| Record {
                    id: id.into(),
                    title: title.into(),
                    description: description.into(),
                })
                .collect())
        })
    }
}
