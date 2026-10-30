//! Memoized calculation facts for an immutable analysis base.

use crate::context::BaseParseContext;
use crate::sem::parameter_resolution::Signature;
use crate::sem::{OverloadRank, TypeReference};
use std::collections::{HashMap, HashSet};

/// Cache state owned by an analysis caller and reused across its requests.
///
/// The caller must associate this cache with one immutable schema base and use
/// it only for calculations against that base. The server owns this association;
/// cache lookups do not validate or fingerprint the environment.
/// It supplements the shared parse context without providing catalog,
/// configuration, or diagnostic services.
pub struct CacheContext {
    cast_sources: HashMap<(TypeReference, bool), HashSet<TypeReference>>,
    argument_ranks: HashMap<(Signature, usize, TypeReference), Option<OverloadRank>>,
    #[cfg(test)]
    cast_source_computations: usize,
    #[cfg(test)]
    rank_computations: usize,
}

impl CacheContext {
    /// Create an empty cache context for an analysis session.
    #[allow(
        clippy::new_without_default,
        reason = "Only new() is exposed to cache owners"
    )]
    pub fn new() -> Self {
        Self {
            cast_sources: HashMap::new(),
            argument_ranks: HashMap::new(),
            #[cfg(test)]
            cast_source_computations: 0,
            #[cfg(test)]
            rank_computations: 0,
        }
    }

    pub(crate) fn cast_sources(
        &mut self,
        context: &impl BaseParseContext,
        target: &TypeReference,
        explicit: bool,
    ) -> HashSet<TypeReference> {
        self.cast_sources
            .entry((target.clone(), explicit))
            .or_insert_with(|| {
                #[cfg(test)]
                {
                    self.cast_source_computations += 1;
                }
                context.cast_sources(target, explicit)
            })
            .clone()
    }

    pub(crate) fn argument_rank(
        &mut self,
        context: &impl BaseParseContext,
        signature: &Signature,
        index: usize,
        source: &TypeReference,
    ) -> Option<OverloadRank> {
        *self
            .argument_ranks
            .entry((signature.clone(), index, source.clone()))
            .or_insert_with(|| {
                #[cfg(test)]
                {
                    self.rank_computations += 1;
                }
                signature.cast_score(context, source, index)
            })
    }
}

#[cfg(test)]
mod tests {
    use crate::context::{CastInfoRead, ParseContext};
    use crate::sem::parameter_resolution::Signature;
    use crate::sem::{PgBuiltInType, TypeReference};
    use crate::test_helpers::{analysis_cache, domain_casts};

    #[rstest::rstest]
    #[case::implicit(false)]
    #[case::explicit(true)]
    fn source_sets_are_reused_and_returned_independently(#[case] explicit: bool) {
        let mut cache = crate::test_helpers::create_cache_context();
        let context = domain_casts::context();
        let target = PgBuiltInType::int4();
        let expected = context.cast_sources(&target, explicit);
        let mut first = cache.cast_sources(&context, &target, explicit);
        assert_eq!(first, expected);
        first.clear();
        assert_eq!(cache.cast_sources(&context, &target, explicit), expected);
        assert_eq!(cache.cast_source_computations, 1);
    }

    #[test]
    fn source_keys_separate_targets_and_cast_selectors() {
        let mut cache = crate::test_helpers::create_cache_context();
        let context = domain_casts::context();
        let target = PgBuiltInType::int4();
        let implicit = cache.cast_sources(&context, &target, false);
        let explicit = cache.cast_sources(&context, &target, true);
        assert_ne!(implicit, explicit);
        assert_eq!(cache.cast_source_computations, 2);
        let other = PgBuiltInType::int8();
        assert_eq!(
            cache.cast_sources(&context, &other, false),
            context.cast_sources(&other, false)
        );
        assert_eq!(cache.cast_source_computations, 3);
    }

    #[test]
    fn empty_source_sets_are_cached() {
        let mut cache = crate::test_helpers::create_cache_context();
        let mut context = domain_casts::context();
        let broken = domain_casts::domain(&mut context, "broken", None);
        assert!(cache.cast_sources(&context, &broken, true).is_empty());
        assert!(cache.cast_sources(&context, &broken, true).is_empty());
        assert_eq!(cache.cast_source_computations, 1);
    }

    #[rstest::rstest]
    #[case::identity(PgBuiltInType::int4())]
    #[case::rejected(PgBuiltInType::bool())]
    fn argument_ranks_cache_success_and_rejection(#[case] source: TypeReference) {
        let mut cache = crate::test_helpers::create_cache_context();
        let context = domain_casts::context();
        let signature = Signature::new(&[PgBuiltInType::int4()], &PgBuiltInType::bool());
        let expected = signature.cast_score(&context, &source, 0);
        assert_eq!(
            cache.argument_rank(&context, &signature, 0, &source),
            expected
        );
        assert_eq!(
            cache.argument_rank(&context, &signature, 0, &source),
            expected
        );
        assert_eq!(cache.rank_computations, 1);
    }

    #[test]
    fn rank_keys_preserve_function_declarations_and_unrestricted_inputs() {
        let mut cache = crate::test_helpers::create_cache_context();
        let context = domain_casts::context();
        let target = PgBuiltInType::int4();
        let concrete = Signature::new_function(
            std::slice::from_ref(&target),
            std::slice::from_ref(&target),
            &[],
            &target,
        )
        .unwrap();
        let polymorphic = Signature::new_function(
            std::slice::from_ref(&target),
            &[TypeReference::from_master_type_name("anyelement")],
            &[],
            &target,
        )
        .unwrap();
        let unrestricted = Signature::new_function(
            std::slice::from_ref(&target),
            std::slice::from_ref(&target),
            &[0],
            &target,
        )
        .unwrap();
        let exact = cache.argument_rank(&context, &concrete, 0, &target);
        let substituted = cache.argument_rank(&context, &polymorphic, 0, &target);
        assert!(exact > substituted);
        let rejected = PgBuiltInType::bool();
        assert!(
            cache
                .argument_rank(&context, &concrete, 0, &rejected)
                .is_none()
        );
        assert!(
            cache
                .argument_rank(&context, &unrestricted, 0, &rejected)
                .is_some()
        );
        assert_eq!(cache.rank_computations, 4);
    }

    #[test]
    fn rank_keys_separate_argument_positions_and_sources() {
        let mut cache = crate::test_helpers::create_cache_context();
        let context = domain_casts::context();
        let signature = Signature::new(
            &[PgBuiltInType::int4(), PgBuiltInType::bool()],
            &PgBuiltInType::bool(),
        );
        let source = PgBuiltInType::int4();
        assert!(
            cache
                .argument_rank(&context, &signature, 0, &source)
                .is_some()
        );
        assert!(
            cache
                .argument_rank(&context, &signature, 1, &source)
                .is_none()
        );
        assert!(
            cache
                .argument_rank(&context, &signature, 1, &PgBuiltInType::bool())
                .is_some()
        );
        assert_eq!(cache.rank_computations, 3);
    }

    #[rstest::rstest]
    #[case::nested("SELECT COALESCE($1::int4, 2) BETWEEN SYMMETRIC 3 AND 1 AS result")]
    #[case::negative("SELECT $1::int4 NOT BETWEEN SYMMETRIC 3 AND 1 AS result")]
    #[case::array("SELECT ARRAY[$1, 1] AS result")]
    #[case::domain("SELECT $1::public.amount AS result")]
    #[case::correlated("SELECT $1 + $2 AS result, $1::int4 AS first")]
    #[case::nested_query("WITH q AS (SELECT $1::int4 AS n) SELECT n + $2 AS result FROM q")]
    fn fresh_query_layers_reuse_facts_and_preserve_analysis(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let base = analysis_cache::base();
        let mut cache = crate::test_helpers::create_cache_context();
        let first = analysis_cache::analyze(sql, &base, &mut cache)?;
        let computations = (cache.cast_source_computations, cache.rank_computations);
        assert!(computations.0 + computations.1 > 0);
        let repeated = analysis_cache::analyze(sql, &base, &mut cache)?;
        assert_eq!(
            (cache.cast_source_computations, cache.rank_computations),
            computations
        );
        let independent =
            analysis_cache::analyze(sql, &base, &mut crate::test_helpers::create_cache_context())?;
        analysis_cache::assert_equivalent(&first, &repeated);
        analysis_cache::assert_equivalent(&first, &independent);
        Ok(())
    }

    #[test]
    fn failed_analysis_does_not_replay_diagnostics_or_constraints() -> testresult::TestResult {
        let base = analysis_cache::base();
        let mut cache = crate::test_helpers::create_cache_context();
        let rejected =
            analysis_cache::analyze("SELECT missing, $1::int4 AS result", &base, &mut cache)?;
        assert!(!rejected.context().problems().is_empty());
        assert!(cache.cast_source_computations > 0);
        let valid = analysis_cache::analyze("SELECT 1 AS result", &base, &mut cache)?;
        assert!(valid.context().problems().is_empty());
        assert!(
            valid
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_none()
        );
        let independent = analysis_cache::analyze(
            "SELECT 1 AS result",
            &base,
            &mut crate::test_helpers::create_cache_context(),
        )?;
        analysis_cache::assert_equivalent(&valid, &independent);
        Ok(())
    }
}
