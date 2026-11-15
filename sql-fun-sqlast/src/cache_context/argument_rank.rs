//! Borrowed lookup inputs; only retained cache entries own type data.

use crate::sem::parameter_resolution::Signature;
use crate::sem::{OverloadRank, TypeReference};
use std::collections::HashMap;

pub(super) struct ArgumentRankInput<'a> {
    signature: &'a Signature,
    index: usize,
    source: &'a TypeReference,
}

impl<'a> ArgumentRankInput<'a> {
    pub(super) const fn new(
        signature: &'a Signature,
        index: usize,
        source: &'a TypeReference,
    ) -> Self {
        Self {
            signature,
            index,
            source,
        }
    }
}

// Optimization history (#1121; measured 2026-11-15 with Rust 1.100.0, release):
// Replacing only the innermost map with TrieMap, using request-local canonical
// CompactString keys for concrete types, increased complete-workload CPU medians
// by 4.5% (default features) / 2.1% (all features), over three runs each. Reduced
// hashing was offset by per-request key formatting and Trie traversal; combining
// it with inline Trie fragments did not recover the regression. Keep borrowed
// structural lookups unless new evidence addresses those costs. Consult the
// evaluation before repeating this candidate with the same representation:
// https://github.com/kazuk/sql-fun/pull/1122#issuecomment-6892389106
#[derive(Default)]
pub(super) struct ArgumentRanks {
    signatures: HashMap<Signature, HashMap<usize, HashMap<TypeReference, Option<OverloadRank>>>>,
}

impl ArgumentRanks {
    pub(super) fn get_or_compute(
        &mut self,
        input: &ArgumentRankInput<'_>,
        compute: impl FnOnce() -> Option<OverloadRank>,
    ) -> Option<OverloadRank> {
        if let Some(result) = self.lookup(input) {
            return result;
        }
        let result = compute();
        self.retain(input, result);
        result
    }

    fn lookup(&self, input: &ArgumentRankInput<'_>) -> Option<Option<OverloadRank>> {
        let positions = self.signatures.get(input.signature)?;
        let sources = positions.get(&input.index)?;
        sources.get(input.source).copied()
    }

    fn retain(&mut self, input: &ArgumentRankInput<'_>, result: Option<OverloadRank>) {
        if let Some(positions) = self.signatures.get_mut(input.signature) {
            positions
                .entry(input.index)
                .or_default()
                .insert(input.source.clone(), result);
        } else {
            self.signatures.insert(
                input.signature.clone(),
                HashMap::from([(input.index, HashMap::from([(input.source.clone(), result)]))]),
            );
        }
    }
}
