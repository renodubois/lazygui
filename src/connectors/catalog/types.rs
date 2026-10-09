use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Record {
    pub id: String,
    pub title: String,
    pub description: String,
}
