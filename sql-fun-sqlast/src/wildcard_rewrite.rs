//! Location-checked wildcard projection replacement in the owning common AST.

mod rows_source;
mod source;

use self::source::{SourcePlans, ordinal_reference, typed_record_reference};
use crate::wildcard_rewrite_error::WildcardRewriteError;
use crate::{
    names::FullName,
    sem::{
        CoalesceExpr, ColumnReferenceExpr, ImplicitCastExpr, TypeCastExpr, TypeReference,
        WildcardQualifiers,
    },
    stack::with_stack,
};

use crate::{
    ast::{
        Call, Expr, Name, Statement, Target, TypeName,
        projection::{ProjectionReplacements, ProjectionScopes},
    },
    sem::{SemScalarExpr, WildcardExpansion},
};

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
    statement: &mut Statement,
    expansions: &[WildcardExpansion],
) -> Result<usize, WildcardRewriteError> {
    let mut staged = statement.clone();
    let mut plan = WildcardRewritePlan::new(expansions);
    plan.prepare(&mut staged)?;
    let count = plan.apply(&mut staged)?;
    *statement = staged;
    Ok(count)
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
            rewrite(expansion).prepare_sources(&scopes, &mut self.sources)?;
            let targets = rewrite(expansion).rewrite_targets()?;
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

impl Rewrite<'_, SemScalarExpr> {
    fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        with_stack(|| match self.value {
            SemScalarExpr::ColumnRef(value) => rewrite(value).projection_expression(bindings),
            SemScalarExpr::Coalesce(value) => rewrite(value).projection_expression(bindings),
            SemScalarExpr::ImplicitCast(value) => rewrite(value).projection_expression(bindings),
            SemScalarExpr::TypeCast(value) => rewrite(value).projection_expression(bindings),
            _ => Err(WildcardRewriteError::invalid(
                "resolved wildcard expression has no executable projection representation",
            )),
        })
    }
    fn projection_cast(
        &self,
        target: &TypeReference,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        if target.type_definition().is_some() {
            return Err(WildcardRewriteError::invalid(
                "wildcard cast target has no SQL type name",
            ));
        }
        let expression = self.projection_expression(bindings)?;
        let target = TypeName::projection(
            &rewrite(
                target
                    .full_name()
                    .expect("owned cast targets rejected above"),
            )
            .projection_name(),
            target.is_array() == Some(true),
        );
        Ok(Expr::projection_cast(&expression, &target))
    }
}
impl Rewrite<'_, ColumnReferenceExpr> {
    fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
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
impl Rewrite<'_, CoalesceExpr> {
    fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        let mut arguments = Vec::new();
        for argument in self.arguments() {
            arguments.push(rewrite(argument).projection_expression(bindings)?);
        }
        Ok(Expr::projection_call(&Call::coalesce_projection(
            &arguments,
        )))
    }
}
impl Rewrite<'_, ImplicitCastExpr> {
    fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        let (argument, target) = self.projection_operand();
        rewrite(argument).projection_cast(target, bindings)
    }
}
impl Rewrite<'_, TypeCastExpr> {
    fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        let (argument, target) = self.projection_operand();
        rewrite(argument).projection_cast(target, bindings)
    }
}

impl Rewrite<'_, WildcardExpansion> {
    fn rewrite_targets(&self) -> Result<Vec<Target>, WildcardRewriteError> {
        let bindings = self.qualifiers();
        let mut targets = Vec::new();
        for (index, column) in self.columns().iter().enumerate() {
            let expression = if let Some(record) = self.typed_record_at(index) {
                typed_record_reference(record)
            } else if let Some((alias, ordinal)) = self.source_ordinal_at(index) {
                ordinal_reference(alias, ordinal)
            } else if let Some(parts) = self.reference_at(index) {
                Expr::projection_column(&Name::from_parts(parts))
            } else {
                let Some(expression) = column.expression() else {
                    return Err(WildcardRewriteError::invalid(
                        "wildcard result has no resolved expression",
                    ));
                };
                rewrite(expression).projection_expression(bindings)?
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

impl Rewrite<'_, FullName> {
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

struct Rewrite<'a, T: ?Sized> {
    value: &'a T,
}
const fn rewrite<T: ?Sized>(value: &T) -> Rewrite<'_, T> {
    Rewrite { value }
}
impl<T: ?Sized> std::ops::Deref for Rewrite<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value
    }
}
