//! Borrowed type inputs with request-local canonical keys for concrete ranks.

use crate::sem::parameter_resolution::Signature;
use crate::{
    TrieMap,
    sem::{OverloadRank, TypeReference},
};
use ::compact_str::CompactString;
use std::collections::HashMap;

pub(super) struct ArgumentRankInput<'a> {
    signature: &'a Signature,
    index: usize,
    source: &'a TypeReference,
    concrete_key: Option<CompactString>,
}

impl<'a> ArgumentRankInput<'a> {
    pub(super) fn new(signature: &'a Signature, index: usize, source: &'a TypeReference) -> Self {
        Self {
            signature,
            index,
            source,
            // Only Concrete Display is an injective type-identity spelling.
            // Keep Dynamic definitions on their original structural HashMap key.
            concrete_key: if source.is_dynamic() {
                None
            } else {
                Some(::compact_str::format_compact!("{source}"))
            },
        }
    }
}

#[derive(Default)]
pub(super) struct ArgumentRanks {
    signatures: HashMap<Signature, HashMap<usize, SourceRanks>>,
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
        sources.lookup(input)
    }

    fn retain(&mut self, input: &ArgumentRankInput<'_>, result: Option<OverloadRank>) {
        if let Some(positions) = self.signatures.get_mut(input.signature) {
            positions
                .entry(input.index)
                .or_default()
                .retain(input, result);
        } else {
            let mut sources = SourceRanks::default();
            sources.retain(input, result);
            self.signatures.insert(
                input.signature.clone(),
                HashMap::from([(input.index, sources)]),
            );
        }
    }
}

/// Source identity storage; cached rejection remains distinct from a missing key.
#[derive(Default)]
struct SourceRanks {
    concrete: TrieMap<Option<OverloadRank>>,
    other: HashMap<TypeReference, Option<OverloadRank>>,
}

impl SourceRanks {
    fn lookup(&self, input: &ArgumentRankInput<'_>) -> Option<Option<OverloadRank>> {
        if let Some(key) = &input.concrete_key {
            self.concrete.get(key).copied()
        } else {
            self.other.get(input.source).copied()
        }
    }

    fn retain(&mut self, input: &ArgumentRankInput<'_>, result: Option<OverloadRank>) {
        if let Some(key) = &input.concrete_key {
            self.concrete.insert(key, result);
        } else {
            self.other.insert(input.source.clone(), result);
        }
    }
}

#[cfg(test)]
mod source_identity_tests {
    use crate::cache_context::argument_rank::{ArgumentRankInput, SourceRanks};
    use crate::{
        names::FullName,
        sem::{
            OverloadRank, PgBuiltInType, TypeDefinition, TypeReference,
            parameter_resolution::Signature,
        },
    };

    #[test]
    fn distinguishes_quoted_array_and_dynamic_identities() {
        let names = [
            FullName::with_schema("a", "item"),
            FullName::with_schema("a", "item[]"),
            FullName::from_local_name("a.item"),
            FullName::with_schema("a.b", "item"),
            FullName::with_schema("a", "b.item"),
            FullName::with_catalog("db", "a", "item"),
            FullName::with_schema("a", "Mixed\"界"),
        ];
        let mut sources = Vec::new();
        for name in &names {
            sources.push(TypeReference::concrete_type_ref(name, false));
            sources.push(TypeReference::concrete_type_ref(name, true));
        }
        // Named Dynamic definitions may share Concrete's display text and may
        // differ structurally despite having the same nominal name.
        sources.push(TypeReference::dynamic(&TypeDefinition::builtin(
            &names[0], None,
        )));
        sources.push(TypeReference::dynamic(&TypeDefinition::builtin(
            &names[0],
            Some(true),
        )));
        let signature = Signature::new(&[PgBuiltInType::int4()], &PgBuiltInType::bool());
        let mut ranks = SourceRanks::default();
        for source in &sources {
            let input = ArgumentRankInput::new(&signature, 0, source);
            assert_eq!(ranks.lookup(&input), None);
            ranks.retain(&input, Some(OverloadRank::default()));
        }
        for source in &sources {
            let input = ArgumentRankInput::new(&signature, 0, source);
            assert_eq!(ranks.lookup(&input), Some(Some(OverloadRank::default())));
            ranks.retain(&input, None);
        }
        for source in &sources {
            let input = ArgumentRankInput::new(&signature, 0, source);
            assert_eq!(ranks.lookup(&input), Some(None));
        }
    }
}
