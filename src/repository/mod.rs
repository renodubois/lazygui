//! Window-local repository transition contract. No tabs or global repository state.
use crate::git::Identity;
pub struct Navigation {
    current: Identity,
    parents: Vec<Identity>,
    generation: u64,
}
impl Navigation {
    pub fn new(current: Identity) -> Self {
        Self {
            current,
            parents: vec![],
            generation: 1,
        }
    }
    pub fn current(&self) -> &Identity {
        &self.current
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn switch_in_place(&mut self, next: Identity) {
        self.current = next;
        self.parents.clear();
        self.generation += 1;
    }
    pub fn enter_submodule(&mut self, next: Identity) {
        self.parents
            .push(std::mem::replace(&mut self.current, next));
        self.generation += 1;
    }
    pub fn return_parent(&mut self) -> bool {
        if let Some(parent) = self.parents.pop() {
            self.current = parent;
            self.generation += 1;
            true
        } else {
            false
        }
    }
    pub fn accepts(&self, generation: u64) -> bool {
        generation == self.generation
    }
}
#[cfg(test)]
#[path = "tests/navigation.rs"]
mod tests;
