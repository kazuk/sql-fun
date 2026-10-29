use crate::context::BaseParseContext;
use crate::sem::{CastContext, TypeReference};

/// Compare exact-match counts before considering preferred conversions.
/// This uses existing cast preferences; category/unknown resolution is separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct OverloadRank {
    original_exact: bool,
    exact: usize,
    preferred: usize,
}

impl Default for OverloadRank {
    fn default() -> Self {
        Self {
            original_exact: true,
            exact: 0,
            preferred: 0,
        }
    }
}

impl OverloadRank {
    // Original exact signatures win first. Only subsequent operator ranking
    // treats domain inputs as their bases; cast eligibility stays separate.
    pub(crate) fn operator_argument(
        catalog: &impl BaseParseContext,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<Self> {
        source.implicit_match_context(target, catalog)?;
        // Eligibility above permits original identity even for an unresolved
        // domain definition; such an identity needs no base-derived evidence.
        let base = source
            .domain_base_type(catalog)
            .unwrap_or_else(|| source.clone());
        let cast = base.implicit_match_context(target, catalog);
        let mut rank = cast.map(Self::from).unwrap_or_default();
        rank.original_exact = source == target;
        Some(rank)
    }

    // An inferred array signature can rank its possible concrete element
    // assignments, but never becomes an original exact operator declaration.
    pub(crate) const fn instantiated_operator(self) -> Self {
        Self {
            original_exact: false,
            ..self
        }
    }

    pub(crate) fn substituted_operator_argument() -> Self {
        Self {
            original_exact: false,
            ..Self::default()
        }
    }

    pub(crate) fn argument(
        catalog: &impl BaseParseContext,
        source: Option<&TypeReference>,
        target: Option<&TypeReference>,
    ) -> Option<Self> {
        let (Some(source), Some(target)) = (source, target) else {
            return Some(Self::default());
        };
        Some(Self::from(source.implicit_match_context(target, catalog)?))
    }

    // Instantiated inputs decide applicability; only the declaration can earn
    // exact/preferred credit. A substituted polymorphic placeholder earns none.
    pub(crate) fn function_argument(
        catalog: &impl BaseParseContext,
        source: Option<&TypeReference>,
        target: Option<&TypeReference>,
        declared_target: Option<&TypeReference>,
    ) -> Option<Self> {
        let rank = Self::argument(catalog, source, target)?;
        if declared_target.is_some_and(|ty| {
            !ty.is_dynamic() && (ty.is_simple_infer_arg() || ty.is_common_infer_arg())
        }) {
            Some(Self::default())
        } else {
            Some(rank)
        }
    }
}

impl From<CastContext> for OverloadRank {
    fn from(context: CastContext) -> Self {
        Self {
            original_exact: true,
            exact: usize::from(context.is_no_conversion()),
            preferred: usize::from(context.is_implicit_preferred()),
        }
    }
}

impl std::ops::Add for OverloadRank {
    type Output = Self;

    fn add(self, right: Self) -> Self {
        Self {
            original_exact: self.original_exact && right.original_exact,
            exact: self.exact + right.exact,
            preferred: self.preferred + right.preferred,
        }
    }
}

#[cfg(test)]
mod test_overload_rank {
    use super::OverloadRank;
    use crate::sem::CastContext;

    #[rstest::rstest]
    #[case::exact_over_preferred(
        CastContext::no_conversion(),
        CastContext::implicit(),
        CastContext::implicit_preferred(),
        CastContext::implicit_preferred()
    )]
    #[case::two_exact(
        CastContext::no_conversion(),
        CastContext::no_conversion(),
        CastContext::no_conversion(),
        CastContext::implicit_preferred()
    )]
    #[case::preferred_tiebreak(
        CastContext::no_conversion(),
        CastContext::implicit_preferred(),
        CastContext::no_conversion(),
        CastContext::implicit()
    )]
    fn stages_cannot_outweigh_exact_matches(
        #[case] left: CastContext,
        #[case] right: CastContext,
        #[case] other_left: CastContext,
        #[case] other_right: CastContext,
    ) {
        assert!(
            OverloadRank::from(left) + OverloadRank::from(right)
                > OverloadRank::from(other_left) + OverloadRank::from(other_right)
        );
    }

    #[test]
    fn argument_order_does_not_break_ties() {
        let exact = OverloadRank::from(CastContext::no_conversion());
        let implicit = OverloadRank::from(CastContext::implicit());
        assert_eq!(exact + implicit, implicit + exact);
    }
}
