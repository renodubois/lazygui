use crate::connectors::catalog::{Record, Reply};

#[derive(Default)]
pub(crate) struct State {
    query: String,
    records: Vec<Record>,
    selected: Option<String>,
    loading: bool,
    error: Option<String>,
    generation: u64,
}
impl State {
    pub(crate) fn query(&self) -> &str {
        &self.query
    }
    pub(crate) fn records(&self) -> &[Record] {
        &self.records
    }
    pub(crate) fn selected(&self) -> Option<&Record> {
        self.records
            .iter()
            .find(|record| Some(&record.id) == self.selected.as_ref())
    }
    pub(crate) fn loading(&self) -> bool {
        self.loading
    }
    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub(super) fn begin(&mut self, query: String) -> u64 {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("request generation exhausted");
        self.query = query;
        self.loading = true;
        self.error = None;
        self.generation
    }
    pub(super) fn complete(&mut self, generation: u64, result: Reply) {
        if generation != self.generation || !self.loading {
            return;
        }
        self.loading = false;
        match result {
            Ok(records) => {
                if !records
                    .iter()
                    .any(|record| Some(&record.id) == self.selected.as_ref())
                {
                    self.selected = records.first().map(|record| record.id.clone());
                }
                self.records = records;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    pub(super) fn select(&mut self, id: &str) {
        if self.records.iter().any(|record| record.id == id) {
            self.selected = Some(id.into());
        }
    }
}
#[cfg(test)]
#[path = "tests/state.rs"]
mod tests;
