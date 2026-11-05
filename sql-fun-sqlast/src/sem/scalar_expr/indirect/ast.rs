use crate::{
    ast::{Access, Dialect, Expr, Slice},
    context::ParseContext,
    sem::{AnalysisError, ArraySubscript, IndirectionExpr, ScalarAnalysisContext, SemScalarExpr},
};

impl IndirectionExpr {
    pub(crate) fn from_ast_sequence(
        items: &[Access],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        mut base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut remaining = items;
        while let Some((first, rest)) = remaining.split_first() {
            let count = if context.dialect() == Dialect::postgres() {
                Self::ast_subscript_prefix_length(remaining)
            } else {
                0
            };
            if count > 1 {
                let (group, rest) = remaining.split_at(count);
                let mut subscripts = Vec::with_capacity(count);
                for access in group {
                    subscripts.push(ArraySubscript::from_ast_access(access, context, scope)?);
                }
                base = Self::resolved_subscripts(context, &base, &subscripts)?;
                remaining = rest;
            } else {
                base = Self::from_ast_access(first, context, scope, base)?;
                remaining = rest;
            }
        }
        Ok(base)
    }
}

impl IndirectionExpr {
    pub(crate) fn ast_subscript_prefix_length(items: &[Access]) -> usize {
        let mut count = 0;
        for access in items {
            if !Self::ast_is_subscript(access) {
                break;
            }
            count += 1;
        }
        count
    }
}

impl IndirectionExpr {
    pub(crate) const fn ast_is_subscript(ast: &Access) -> bool {
        matches!(ast, Access::Index(_) | Access::Slice(_))
    }
}

impl IndirectionExpr {
    pub(crate) fn from_ast_access(
        ast: &Access,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match ast {
            Access::Field(name) => Self::resolved_member(context, base, name),
            Access::Index(index) => Self::from_ast_index(index, context, scope, base),
            Access::Slice(slice) => Self::from_ast_slice(slice, context, scope, base),
            Access::Star => SemScalarExpr::ast_invalid_star_projection(context),
        }
    }
}

impl IndirectionExpr {
    fn from_ast_index(
        index: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let index = SemScalarExpr::from_ast_in(index, context, scope)?;
        Self::resolved_index(context, base, index)
    }

    fn from_ast_slice(
        slice: &Slice,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let bounds = ArraySubscript::from_ast_slice(slice, context, scope)?;
        Self::resolved_slice(
            context,
            base,
            bounds.lower().cloned(),
            bounds.upper().cloned(),
        )
    }
}
