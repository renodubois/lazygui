use super::*;
use crate::{connectors::catalog::Error, test_support::catalog::record};

#[test]
fn stale_and_duplicate_results_do_not_replace_current_state() {
    let mut state = State::default();
    let old = state.begin("old".into());
    let current = state.begin("current".into());
    state.complete(old, Ok(vec![record("old")]));
    assert!(state.loading());
    state.complete(current, Ok(vec![record("new")]));
    state.complete(current, Err(Error::Transport));
    assert_eq!(state.selected().unwrap().id, "new");
    assert!(state.error().is_none());
}
#[test]
fn selection_survives_reload_by_identity_and_empty_results_clear_it() {
    let mut state = State::default();
    let generation = state.begin(String::new());
    state.complete(generation, Ok(vec![record("a"), record("b")]));
    state.select("b");
    state.select("unknown");
    let generation = state.begin(String::new());
    state.complete(generation, Ok(vec![record("b"), record("a")]));
    assert_eq!(state.selected().unwrap().id, "b");
    let generation = state.begin(String::new());
    state.complete(generation, Ok(vec![]));
    assert!(state.selected().is_none());
}
#[test]
fn failure_preserves_last_successful_records_and_retry_clears_error() {
    let mut state = State::default();
    let generation = state.begin(String::new());
    state.complete(generation, Ok(vec![record("a")]));
    let generation = state.begin("failing".into());
    state.complete(generation, Err(Error::Transport));
    assert!(state.error().is_some());
    assert_eq!(state.records().len(), 1);
    state.begin("retry".into());
    assert!(state.error().is_none());
}
