//! Location-checked wildcard projection replacement in the owning common AST.

mod error;
mod rows_source;
mod source;

pub use self::error::WildcardRewriteError;
use self::source::{SourcePlans, ordinal_reference, typed_record_reference};
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

impl WildcardExpansion {
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
    pub fn rewrite(
        expansions: &[Self],
        statement: &mut Statement,
    ) -> Result<usize, WildcardRewriteError> {
        let mut staged = statement.clone();
        let mut plan = WildcardRewritePlan::new(expansions);
        plan.prepare(&mut staged)?;
        let count = plan.apply(&mut staged)?;
        *statement = staged;
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
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        with_stack(|| match self {
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
            &projection_name(
                target
                    .full_name()
                    .expect("owned cast targets rejected above"),
            ),
            target.is_array() == Some(true),
        );
        Ok(Expr::projection_cast(&expression, &target))
    }
}
impl ColumnReferenceExpr {
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
impl CoalesceExpr {
    fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
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
impl ImplicitCastExpr {
    fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        let (argument, target) = self.projection_operand();
        argument.projection_cast(target, bindings)
    }
}
impl TypeCastExpr {
    fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
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

fn projection_name(name: &FullName) -> Name {
    let mut parts = Vec::new();
    if let Some(catalog) = name.catalog() {
        parts.push(catalog.to_string());
    }
    if let Some(schema) = name.schema() {
        parts.push(schema.to_string());
    }
    parts.push(name.local_name());
    Name::from_parts(&parts)
}
