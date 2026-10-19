use crate::sem::{BaseParseContext, CastContext, TypeReference};

/// Exact matches are eliminated before preferred conversions are considered.
/// This uses existing cast preferences; category/unknown resolution is separate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct OverloadRank {
    exact: usize,
    preferred: usize,
}

impl OverloadRank {
    pub(crate) fn argument(
        catalog: &impl BaseParseContext,
        source: Option<&TypeReference>,
        target: Option<&TypeReference>,
    ) -> Option<Self> {
        let (Some(source), Some(target)) = (source, target) else {
            return Some(Self::default());
        };
        Some(Self::from(
            catalog.get_implicit_cast(source, target)?.context(),
        ))
    }
}

impl From<CastContext> for OverloadRank {
    fn from(context: CastContext) -> Self {
        Self {
            exact: usize::from(context.is_no_conversion()),
            preferred: usize::from(context.is_implicit_preferred()),
        }
    }
}

impl std::ops::Add for OverloadRank {
    type Output = Self;

    fn add(self, right: Self) -> Self {
        Self {
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
