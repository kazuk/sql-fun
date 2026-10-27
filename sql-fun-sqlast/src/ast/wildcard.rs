//! Location-checked wildcard projection replacement in the owning common AST.

mod rows_source;
mod source;
use self::source::SourcePlans;

use crate::ast::projection::{
    ProjectionError, ProjectionLocations, ProjectionReplacements, ProjectionScopes,
};
use crate::ast::{Call, Expr, Name, Statement, Target, TypeName};
use crate::sem::{SemScalarExpr, WildcardExpansion};

/// A rejected wildcard rewrite. The statement remains unchanged on failure.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct WildcardRewriteError(RewriteProblem);

#[derive(Debug, thiserror::Error)]
enum RewriteProblem {
    #[error("wildcard expansion has no lexical location")]
    MissingLocation,
    #[error("wildcard location was not found")]
    NotFound,
    #[error("wildcard location is ambiguous")]
    Ambiguous,
    #[error("duplicate wildcard expansion location")]
    Duplicate,
    #[error("lexical location does not identify a wildcard projection")]
    WrongKind,
    #[error("wildcard replacement would remove the complete RETURNING projection")]
    EmptyReturning,
    #[error("{0}")]
    InvalidExpression(String),
}

impl RewriteProblem {
    const fn missing_location() -> Self {
        Self::MissingLocation
    }
    const fn is_missing_location(&self) -> bool {
        matches!(self, Self::MissingLocation)
    }
    const fn not_found() -> Self {
        Self::NotFound
    }
    const fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound)
    }
    const fn ambiguous() -> Self {
        Self::Ambiguous
    }
    const fn is_ambiguous(&self) -> bool {
        matches!(self, Self::Ambiguous)
    }
    const fn duplicate_location() -> Self {
        Self::Duplicate
    }
    const fn is_duplicate_location(&self) -> bool {
        matches!(self, Self::Duplicate)
    }
    const fn wrong_node_kind() -> Self {
        Self::WrongKind
    }
    const fn is_wrong_node_kind(&self) -> bool {
        matches!(self, Self::WrongKind)
    }
    const fn empty_returning() -> Self {
        Self::EmptyReturning
    }
    const fn is_empty_returning(&self) -> bool {
        matches!(self, Self::EmptyReturning)
    }
    fn invalid(message: &str) -> Self {
        Self::InvalidExpression(message.to_owned())
    }
    const fn is_invalid_expression(&self) -> bool {
        matches!(self, Self::InvalidExpression(_))
    }
}

impl WildcardRewriteError {
    fn invalid(message: &str) -> Self {
        Self(RewriteProblem::invalid(message))
    }
    const fn missing_location() -> Self {
        Self(RewriteProblem::missing_location())
    }
    /// Whether an expansion has no parser-provided location.
    pub const fn is_missing_location(&self) -> bool {
        self.0.is_missing_location()
    }
    const fn not_found() -> Self {
        Self(RewriteProblem::not_found())
    }
    /// Whether no projection exists at the supplied location.
    pub const fn is_not_found(&self) -> bool {
        self.0.is_not_found()
    }
    const fn ambiguous() -> Self {
        Self(RewriteProblem::ambiguous())
    }
    /// Whether multiple projection targets share the supplied location.
    pub const fn is_ambiguous(&self) -> bool {
        self.0.is_ambiguous()
    }
    const fn duplicate_location() -> Self {
        Self(RewriteProblem::duplicate_location())
    }
    /// Whether two expansion groups request the same location.
    pub const fn is_duplicate_location(&self) -> bool {
        self.0.is_duplicate_location()
    }
    const fn wrong_node_kind() -> Self {
        Self(RewriteProblem::wrong_node_kind())
    }
    /// Whether the location identifies a projection other than a wildcard.
    pub const fn is_wrong_node_kind(&self) -> bool {
        self.0.is_wrong_node_kind()
    }
    const fn empty_returning() -> Self {
        Self(RewriteProblem::empty_returning())
    }
    /// Whether a replacement would erase the complete RETURNING projection.
    pub const fn is_empty_returning(&self) -> bool {
        self.0.is_empty_returning()
    }
    /// Whether a resolved output cannot be emitted as an explicit projection.
    pub const fn is_invalid_expression(&self) -> bool {
        self.0.is_invalid_expression()
    }
}

impl Statement {
    /// Copy target locations from a parser round trip of this statement.
    ///
    /// The source must render identically in the same PostgreSQL dialect and
    /// retain parser locations for every target. Only target locations change;
    /// expressions and diagnostic anchors remain attached to their original input.
    /// The consumed source is temporary parser output. All checks precede edits.
    ///
    /// # Errors
    /// Returns an error without changing this statement if rendering, statement
    /// correspondence, or parser target locations cannot be validated.
    pub fn synchronize_target_locations(
        &mut self,
        mut reparsed: Self,
    ) -> Result<(), WildcardRewriteError> {
        if self.dialect != crate::ast::Dialect::postgres() || self.dialect != reparsed.dialect {
            return Err(WildcardRewriteError::invalid(
                "target location synchronization requires the same PostgreSQL dialect",
            ));
        }
        let original = self
            .deparse_sql(self.dialect)
            .map_err(|error| WildcardRewriteError::invalid(&error.to_string()))?;
        let rendered = reparsed
            .deparse_sql(reparsed.dialect)
            .map_err(|error| WildcardRewriteError::invalid(&error.to_string()))?;
        if original != rendered {
            return Err(WildcardRewriteError::invalid(
                "target location source does not render as the destination statement",
            ));
        }
        let locations = ProjectionLocations::capture(&mut reparsed.node);
        locations
            .validate_source()
            .map_err(WildcardRewriteError::from)?;
        locations
            .validate_destination(&mut self.node)
            .map_err(WildcardRewriteError::from)?;
        locations.apply(&mut self.node);
        Ok(())
    }

    /// Replace table-dependent wildcard projections using one semantic analysis.
    /// Returns the number of replaced projection items.
    ///
    /// Expansions must originate from this same, unmodified statement. Lexical
    /// locations are relative to the PostgreSQL parser input. All lookups
    /// and replacement expressions are checked before any edit is committed;
    /// an error leaves the complete statement unchanged and allows retry.
    /// Unrelated wildcard expansions are ignored. An empty expansion removes its
    /// projection item, including the final item in a zero-column SELECT.
    ///
    /// # Errors
    /// Rejects missing/ambiguous locations, non-wildcard destinations, and
    /// unresolved expressions that cannot represent an executable projection.
    pub fn rewrite_wildcards(
        &mut self,
        expansions: &[WildcardExpansion],
    ) -> Result<usize, WildcardRewriteError> {
        let mut staged = self.clone();
        let mut plan = WildcardRewritePlan::new(expansions);
        plan.prepare(&mut staged)?;
        let count = plan.apply(&mut staged)?;
        *self = staged;
        Ok(count)
    }
}

struct WildcardRewritePlan<'a> {
    expansions: &'a [WildcardExpansion],
    sources: SourcePlans,
    targets: ProjectionReplacements,
}
impl<'a> WildcardRewritePlan<'a> {
    const fn new(expansions: &'a [WildcardExpansion]) -> Self {
        Self {
            expansions,
            sources: SourcePlans::new(),
            targets: ProjectionReplacements::new(),
        }
    }
    fn prepare(&mut self, statement: &mut Statement) -> Result<(), WildcardRewriteError> {
        let scopes = ProjectionScopes::capture(&mut statement.node);
        for expansion in self.expansions {
            if !expansion.is_table_dependent() {
                continue;
            }
            let Some(location) = expansion.location() else {
                return Err(WildcardRewriteError::missing_location());
            };
            expansion.prepare_sources(&scopes, &mut self.sources)?;
            let targets = expansion.rewrite_targets()?;
            self.targets
                .add(location, &targets)
                .map_err(WildcardRewriteError::from)?;
        }
        self.targets
            .validate_wildcards(&mut statement.node)
            .map_err(WildcardRewriteError::from)
    }
    fn apply(&mut self, statement: &mut Statement) -> Result<usize, WildcardRewriteError> {
        let count = self.targets.len();
        self.sources.stage(&mut statement.node)?;
        self.targets.apply(&mut statement.node);
        Ok(count)
    }
}

#[cfg(test)]
mod tests;

impl SemScalarExpr {
    fn projection_expression(
        &self,
        bindings: &crate::sem::WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        crate::sem::with_stack(|| match self {
            Self::ColumnRef(value) => value.projection_expression(bindings),
            Self::Coalesce(value) => value.projection_expression(bindings),
            Self::ImplicitCast(value) => value.projection_expression(bindings),
            Self::TypeCast(value) => value.projection_expression(bindings),
            _ => Err(WildcardRewriteError::invalid(
                "resolved wildcard expression has no executable projection representation",
            )),
        })
    }
    fn projection_cast(
        &self,
        target: &crate::sem::TypeReference,
        bindings: &crate::sem::WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        if target.type_definition().is_some() {
            return Err(WildcardRewriteError::invalid(
                "wildcard cast target has no SQL type name",
            ));
        }
        let expression = self.projection_expression(bindings)?;
        let target = TypeName::projection(
            &target.full_name().projection_name(),
            target.is_array() == Some(true),
        );
        Ok(Expr::projection_cast(&expression, &target))
    }
}
impl crate::sem::ColumnReferenceExpr {
    fn projection_expression(
        &self,
        bindings: &crate::sem::WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        if let Some(table) = self.table_column_reference()
            && let Some(qualifier) = bindings.reference_for(table.relation(), table.alias())
        {
            let mut parts = qualifier.to_vec();
            parts.push(self.get_column_name().to_string());
            return Ok(Expr::projection_column(&Name::from_parts(&parts)));
        }
        let Some((alias, column)) = self.projection_reference() else {
            return Err(WildcardRewriteError::invalid(
                "wildcard column binding is unresolved",
            ));
        };
        let parts = if alias.is_empty() {
            vec![column.to_string()]
        } else {
            vec![alias.to_owned(), column.to_string()]
        };
        Ok(Expr::projection_column(&Name::from_parts(&parts)))
    }
}
impl crate::sem::CoalesceExpr {
    fn projection_expression(
        &self,
        bindings: &crate::sem::WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        let mut arguments = Vec::new();
        for argument in self.arguments() {
            arguments.push(argument.projection_expression(bindings)?);
        }
        Ok(Expr::projection_call(&Call::coalesce_projection(
            &arguments,
        )))
    }
}
impl crate::sem::ImplicitCastExpr {
    fn projection_expression(
        &self,
        bindings: &crate::sem::WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        let (argument, target) = self.projection_operand();
        argument.projection_cast(target, bindings)
    }
}
impl crate::sem::TypeCastExpr {
    fn projection_expression(
        &self,
        bindings: &crate::sem::WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        let (argument, target) = self.projection_operand();
        argument.projection_cast(target, bindings)
    }
}

impl WildcardExpansion {
    fn rewrite_targets(&self) -> Result<Vec<Target>, WildcardRewriteError> {
        let bindings = self.qualifiers();
        let mut targets = Vec::new();
        for (index, column) in self.columns().iter().enumerate() {
            let expression = if let Some(record) = self.typed_record_at(index) {
                self::source::typed_record_reference(record)
            } else if let Some((alias, ordinal)) = self.source_ordinal_at(index) {
                self::source::ordinal_reference(alias, ordinal)
            } else if let Some(parts) = self.reference_at(index) {
                Expr::projection_column(&Name::from_parts(parts))
            } else {
                let Some(expression) = column.expression() else {
                    return Err(WildcardRewriteError::invalid(
                        "wildcard result has no resolved expression",
                    ));
                };
                expression.projection_expression(bindings)?
            };
            let Some(alias) = column.column_name() else {
                return Err(WildcardRewriteError::invalid(
                    "wildcard result has no output name",
                ));
            };
            targets.push(Target::projection(&expression, alias.as_str()));
        }
        Ok(targets)
    }
}

impl crate::sem::FullName {
    pub(super) fn projection_name(&self) -> Name {
        let mut parts = Vec::new();
        if let Some(catalog) = self.catalog() {
            parts.push(catalog.to_string());
        }
        if let Some(schema) = self.schema() {
            parts.push(schema.to_string());
        }
        parts.push(self.local_name());
        Name::from_parts(&parts)
    }
}

impl From<ProjectionError> for WildcardRewriteError {
    fn from(error: ProjectionError) -> Self {
        if error.is_missing() {
            Self::not_found()
        } else if error.is_ambiguous() {
            Self::ambiguous()
        } else if error.is_duplicate() {
            Self::duplicate_location()
        } else if error.is_wrong_kind() {
            Self::wrong_node_kind()
        } else if error.is_empty_returning() {
            Self::empty_returning()
        } else {
            Self::invalid("target collections differ after the parser round trip")
        }
    }
}
