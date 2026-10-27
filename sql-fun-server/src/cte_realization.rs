//! Realize resolved catalog queries without catalog lookup or transport work.
//!
//! Catalog CTEs precede existing WITH entries. Their original placeholder-token
//! order, followed by the entire base query, defines the binding contract passed
//! downstream to analysis responses and `LockfileEntry.bind_parameters`.

use ::sql_fun_core::{ParameterName, PlaceholderTypeHint, PlaceholderedSql, ResolvedCteCatalog};
use ::sql_fun_sqlast::{
    ast::{CteDefinition, Dialect, HaveWithClause, Statement},
    parse_statements,
};

/// The complete successful realization and its definitive binding order.
#[derive(Debug)]
pub(crate) struct RealizedCteQuery {
    statement: Statement,
    bindings: OrderedBindings,
}

impl RealizedCteQuery {
    pub(crate) const fn statement(&self) -> &Statement {
        &self.statement
    }

    pub(crate) const fn bindings(&self) -> &OrderedBindings {
        &self.bindings
    }
}

#[derive(Debug)]
pub(crate) struct OrderedBindings(Vec<ResolvedBinding>);

impl OrderedBindings {
    fn from_catalog(
        base_query: &PlaceholderedSql,
        catalog: &ResolvedCteCatalog,
    ) -> Result<Self, CteRealizationError> {
        let mut result = Self(Vec::new());
        catalog.try_map_cte_queries(|_, query| result.include_query(query))?;
        result.include_query(base_query)?;
        Ok(result)
    }

    fn include_query(&mut self, query: &PlaceholderedSql) -> Result<(), CteRealizationError> {
        // parameters() retains first token occurrence order within each SQL piece.
        for (name, hints) in query.parameters() {
            let binding =
                if let Some(index) = self.0.iter().position(|binding| binding.name() == name) {
                    &mut self.0[index]
                } else {
                    self.0.push(ResolvedBinding::new(name));
                    self.0.last_mut().expect("just inserted a binding")
                };
            binding.include_hints(hints)?;
        }
        Ok(())
    }

    fn parameter_names(&self) -> Vec<&ParameterName> {
        self.0.iter().map(ResolvedBinding::name).collect()
    }

    /// Explicit hints aligned one-for-one with the numbered parameter order.
    pub(crate) fn type_hints(&self) -> Vec<Option<String>> {
        self.0
            .iter()
            .map(|binding| binding.type_hint.clone())
            .collect()
    }

    /// Iteration order is the one-based numbered-placeholder binding contract.
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &ResolvedBinding> {
        self.0.iter()
    }
}

#[derive(Debug)]
pub(crate) struct ResolvedBinding {
    name: ParameterName,
    type_hint: Option<String>,
}

impl ResolvedBinding {
    fn new(name: &ParameterName) -> Self {
        Self {
            name: name.clone(),
            type_hint: None,
        }
    }

    fn include_hints(&mut self, hints: &[PlaceholderTypeHint]) -> Result<(), CteRealizationError> {
        for hint in hints {
            let value = hint.hint_value();
            if let Some(existing) = &self.type_hint {
                if existing != value {
                    let conflict = ParameterHintConflict::new(&self.name, existing, value);
                    return Err(CteRealizationError::from_source(
                        "parameter hints",
                        conflict,
                    ));
                }
            } else {
                self.type_hint = Some(value.to_owned());
            }
        }
        Ok(())
    }

    pub(crate) const fn name(&self) -> &ParameterName {
        &self.name
    }
}

/// Explicit hint strings disagree for one logical numbered parameter.
#[derive(Debug, thiserror::Error)]
#[error("parameter {name:?} has conflicting explicit type hints {first:?} and {second:?}")]
pub(crate) struct ParameterHintConflict {
    name: ParameterName,
    first: String,
    second: String,
}

impl ParameterHintConflict {
    fn new(name: &ParameterName, first: &str, second: &str) -> Self {
        Self {
            name: name.clone(),
            first: first.to_owned(),
            second: second.to_owned(),
        }
    }

    pub(crate) const fn name(&self) -> &ParameterName {
        &self.name
    }
    pub(crate) fn first_hint(&self) -> &str {
        &self.first
    }
    pub(crate) fn second_hint(&self) -> &str {
        &self.second
    }
}

/// Contextual failure retaining the original render, parse, or mutation error.
#[derive(Debug, thiserror::Error)]
#[error("{piece}: {source}")]
pub(crate) struct CteRealizationError {
    piece: String,
    #[source]
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl CteRealizationError {
    fn from_source(piece: &str, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self {
            piece: piece.to_owned(),
            source: Box::new(source),
        }
    }

    fn invalid_input(piece: &str, message: &str) -> Self {
        Self::from_source(
            piece,
            std::io::Error::new(std::io::ErrorKind::InvalidInput, message),
        )
    }
}

/// Render with one shared order, parse exactly one statement per piece, and
/// prepend catalog CTEs structurally. The supplied catalog is never modified.
/// Existing recursion flags are preserved; dependency expansion does not enable
/// SQL recursion. Missing inputs and validation failures are returned directly.
pub(crate) fn realize_cte_catalog(
    base_query: &PlaceholderedSql,
    catalog: &ResolvedCteCatalog,
    dialect: Dialect,
) -> Result<RealizedCteQuery, CteRealizationError> {
    let bindings = OrderedBindings::from_catalog(base_query, catalog)?;
    let order = bindings.parameter_names();
    let mut statement = parse_piece(base_query, dialect, &order, "base query")?;
    let definitions = catalog.try_map_cte_queries(|alias, query| {
        let piece = format!("CTE {alias}");
        let body = parse_piece(query, dialect, &order, &piece)?;
        CteDefinition::new(alias.as_str(), &body)
            .map_err(|error| CteRealizationError::from_source(&piece, error))
    })?;
    if !definitions.is_empty() {
        let mut with = statement
            .ensure_with_clause()
            .map_err(|error| CteRealizationError::from_source("base query", error))?;
        for (index, definition) in definitions.into_iter().enumerate() {
            with.insert(index, &definition)
                .map_err(|error| CteRealizationError::from_source(definition.name(), error))?;
        }
    }
    Ok(RealizedCteQuery {
        statement,
        bindings,
    })
}

#[cfg(test)]
mod tests;

fn parse_piece(
    query: &PlaceholderedSql,
    dialect: Dialect,
    order: &[&ParameterName],
    piece: &str,
) -> Result<Statement, CteRealizationError> {
    let sql = dialect
        .render_placeholdered_sql(query, order)
        .map_err(|error| CteRealizationError::from_source(piece, error))?;
    let mut statement = None;
    for parsed in parse_statements(&sql, dialect) {
        let parsed = parsed.map_err(|error| CteRealizationError::from_source(piece, error))?;
        if statement.replace(parsed).is_some() {
            return Err(CteRealizationError::invalid_input(
                piece,
                "expected exactly one SQL statement",
            ));
        }
    }
    statement.ok_or_else(|| {
        CteRealizationError::invalid_input(piece, "expected exactly one SQL statement")
    })
}
