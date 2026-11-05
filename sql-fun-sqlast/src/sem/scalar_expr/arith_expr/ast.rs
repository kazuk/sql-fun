use crate::{
    ast::{Dialect, Name, OperatorKind},
    context::ParseContext,
    names::OperatorName,
    sem::{AnalysisError, ArithExpr, SemScalarExpr},
};

impl ArithExpr {
    pub(crate) const fn ast_is_pattern(ast: &OperatorKind) -> bool {
        matches!(
            ast,
            OperatorKind::Like
                | OperatorKind::NotLike
                | OperatorKind::ILike
                | OperatorKind::Similar
        )
    }
}

impl ArithExpr {
    pub(crate) const fn ast_is_not_like(ast: &OperatorKind) -> bool {
        matches!(ast, OperatorKind::NotLike)
    }
}

impl ArithExpr {
    pub(crate) fn ast_membership_negated(ast: &OperatorKind, name: &Name) -> Option<bool> {
        match ast {
            OperatorKind::In => Some(name.parts() == ["<>"]),
            OperatorKind::NotIn => Some(true),
            _ => None,
        }
    }
}

impl ArithExpr {
    pub(crate) const fn ast_operator_distinctness(ast: &OperatorKind) -> Option<bool> {
        match ast {
            OperatorKind::Distinct => Some(false),
            OperatorKind::NotDistinct => Some(true),
            _ => None,
        }
    }
}

impl ArithExpr {
    pub(crate) const fn ast_has_named_operator(ast: &OperatorKind) -> bool {
        matches!(
            ast,
            OperatorKind::Ordinary
                | OperatorKind::Any
                | OperatorKind::All
                | OperatorKind::Like
                | OperatorKind::NotLike
                | OperatorKind::ILike
                | OperatorKind::Similar
        )
    }
}

impl ArithExpr {
    pub(crate) const fn ast_without_catalog_operator(ast: &OperatorKind) -> bool {
        matches!(ast, OperatorKind::Between | OperatorKind::NotBetween)
    }
}

impl ArithExpr {
    pub(crate) fn from_ast_operator_kind(
        ast: &OperatorKind,
        context: &mut impl ParseContext,
        name: &OperatorName,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match ast {
            OperatorKind::Ordinary => Ok(Self::build_op(context, name, left, right)?.0),
            OperatorKind::Like
            | OperatorKind::NotLike
            | OperatorKind::ILike
            | OperatorKind::Similar => Self::pattern(context, name, left, right),
            OperatorKind::Any => Ok(Self::build_op_any_resolved(context, name, left, right)?.0),
            OperatorKind::All => Ok(Self::build_op_all_resolved(context, name, left, right)?.0),
            OperatorKind::NullIf => Ok(Self::build_null_if(context, left, right)?.0),
            OperatorKind::Between | OperatorKind::NotBetween => {
                Self::from_ast_between(ast, context, left, right)
            }
            OperatorKind::BetweenSymmetric | OperatorKind::NotBetweenSymmetric => {
                Self::from_ast_symmetric_between(ast, context, left, right)
            }
            _ => todo!("operator semantic analysis"),
        }
    }
}

impl ArithExpr {
    pub(crate) fn from_ast_between(
        ast: &OperatorKind,
        context: &mut impl ParseContext,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let left = left.expect("BETWEEN argument");
        let values = right.expect("BETWEEN bounds").into_list_values()?;
        let Ok([start, end]): Result<[SemScalarExpr; 2], _> = values.try_into() else {
            return AnalysisError::raise_unexpected_input("BETWEEN bounds");
        };
        if context.dialect() == Dialect::postgres() {
            return Self::ordinary_between(
                context,
                &left,
                &start,
                &end,
                matches!(ast, OperatorKind::NotBetween),
            );
        }
        Ok(if matches!(ast, OperatorKind::NotBetween) {
            Self::not_between(left, (start, end))
        } else {
            Self::between(left, (start, end))
        })
    }
}

impl ArithExpr {
    pub(crate) fn from_ast_symmetric_between(
        ast: &OperatorKind,
        context: &mut impl ParseContext,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let left = left.expect("BETWEEN SYMMETRIC argument");
        let values = right
            .expect("BETWEEN SYMMETRIC bounds")
            .into_list_values()?;
        let Ok([start, end]): Result<[SemScalarExpr; 2], _> = values.try_into() else {
            return AnalysisError::raise_unexpected_input("BETWEEN SYMMETRIC bounds");
        };
        Self::symmetric_between(
            context,
            &left,
            &start,
            &end,
            matches!(ast, OperatorKind::NotBetweenSymmetric),
        )
    }
}

impl ArithExpr {
    pub(crate) const fn ast_is_null_if(ast: &OperatorKind) -> bool {
        matches!(ast, OperatorKind::NullIf)
    }
}
