use crate::StringSpan;
use crate::ast::Dialect;
use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{
    AnalysisError, AnalysisProblem, CatalogNameLookup, FullName, MinMaxExpr, ParseContext,
    SemScalarExpr, TypeReference, problem::CollateProblem,
};

/// An explicit COLLATE annotation with its completed name lookup and operand.
/// Expression-wide collation derivation is a separate semantic analysis.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CollateExpr {
    argument: Box<SemScalarExpr>,
    collation: CatalogNameLookup,
    source_span: Option<StringSpan>,
    checked_type: Option<TypeReference>,
}

impl CollateExpr {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        argument: &SemScalarExpr,
        name: &FullName,
        source_span: &Option<StringSpan>,
    ) -> Result<Self, AnalysisError> {
        let collation = if context.dialect() == Dialect::sqlite() {
            if let Some(resolved) = context.resolve_sqlite_collation(name) {
                CatalogNameLookup::found(name, &resolved)
            } else {
                CatalogNameLookup::missing(name)
            }
        } else {
            context.lookup_collation(name)
        };
        if collation.is_missing() {
            context.report_analysis_problem(AnalysisProblem::collation_name_not_found(
                &name.to_string(),
            ))?;
        } else if collation.ambiguous_candidates().is_some() {
            context.report_analysis_problem(AnalysisProblem::collate(
                CollateProblem::ambiguous(&name.to_string()),
            ))?;
        }
        let mut value = Self {
            argument: Box::new(argument.clone()),
            collation,
            source_span: *source_span,
            checked_type: None,
        };
        value.validate_type(context)?;
        Ok(value)
    }

    /// Analyzed operand, including nested explicit annotations and failed bindings.
    pub const fn argument(&self) -> &SemScalarExpr {
        &self.argument
    }

    /// Completed collation lookup, retaining requested spelling and failure state.
    pub const fn collation(&self) -> &CatalogNameLookup {
        &self.collation
    }

    /// Parser-provided location of the COLLATE annotation, when available.
    pub const fn source_span(&self) -> Option<&StringSpan> {
        self.source_span.as_ref()
    }

    pub(crate) fn validate_type(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if context.dialect() != Dialect::postgres() {
            return Ok(());
        }
        let Some(ty) = MinMaxExpr::input_type(&self.argument) else {
            return Ok(());
        };
        // Unknown literals are legal here; resolving their type is the owning
        // expression's job. Missing catalog facts do not prove inapplicability.
        if ty == MinMaxExpr::unknown() || self.checked_type.as_ref() == Some(&ty) {
            return Ok(());
        }
        if let Some(collatable) = ty.collation_capability(context) {
            if !collatable {
                context.report_analysis_problem(AnalysisProblem::collate(
                    CollateProblem::operand_type(&ty),
                ))?;
            }
            self.checked_type = Some(ty);
        }
        Ok(())
    }

    pub(super) const fn canonicalize_grouping_key(&mut self) {
        self.source_span = None;
    }

    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.argument.walk(visitor)
    }
}

#[cfg(test)]
mod tests;

impl SemScalarExprNode for CollateExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.argument.get_type()
    }

    fn is_not_null(&self) -> Option<bool> {
        self.argument.is_not_null()
    }
}
