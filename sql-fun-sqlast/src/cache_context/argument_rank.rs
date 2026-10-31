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
