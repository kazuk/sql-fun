//! The shared namespace for explicitly named relation constraints.
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct ConstraintNames {
    occurrences: HashMap<String, usize>,
    first_duplicate: Option<String>,
}

impl ConstraintNames {
    /// Reserve a catalog name. Unnamed constraints never conflict.
    pub(crate) fn reserve(&mut self, name: &str) -> bool {
        if name.is_empty() {
            return true;
        }
        let count = self.occurrences.entry(name.to_owned()).or_default();
        *count += 1;
        if *count > 1 {
            if self.first_duplicate.is_none() {
                self.first_duplicate = Some(name.to_owned());
            }
            return false;
        }
        true
    }

    pub(crate) fn is_duplicate(&self, name: &str) -> bool {
        self.occurrences.get(name).is_some_and(|count| *count > 1)
    }

    pub(crate) fn first_duplicate(&self) -> Option<&str> {
        self.first_duplicate.as_deref()
    }
}
