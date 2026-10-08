//! Realize resolved catalog queries without catalog lookup or transport work.
//!
//! Catalog CTEs precede existing WITH entries. Their original placeholder-token
//! order, followed by the entire base query, defines the binding contract passed
//! downstream to analysis responses and `LockfileEntry.bind_parameters`.

use ::sql_fun_core::{
    CteParameterBindingCollection, ParameterName, PlaceholderedSql, ResolvedCteCatalog,
};
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
    fn from_catalog(catalog: &ResolvedCteCatalog) -> Result<Self, CteRealizationError> {
        let mut bindings = Vec::new();
        let mut names = Vec::new();
        catalog.append_cte_parameter_order(&mut names);
        catalog.base_query().append_parameter_order(&mut names);
        for name in names {
            let Some(metadata) = catalog.parameters().binding(&name) else {
                return Err(CteRealizationError::invalid_input(
                    "binding metadata",
                    &format!("required parameter {name:?} is absent from the resolved catalog"),
                ));
            };
            bindings.push(ResolvedBinding::new(&name, metadata));
        }
        Ok(Self(bindings))
    }

    fn parameter_names(&self) -> Vec<&ParameterName> {
        self.0.iter().map(ResolvedBinding::name).collect()
    }

    /// Iteration order is the one-based numbered-placeholder binding contract.
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &ResolvedBinding> {
        self.0.iter()
    }
}

#[derive(Debug)]
pub(crate) struct ResolvedBinding {
    name: ParameterName,
    metadata: CteParameterBindingCollection,
}

impl ResolvedBinding {
    fn new(name: &ParameterName, metadata: &CteParameterBindingCollection) -> Self {
        Self {
            name: name.clone(),
            metadata: metadata.clone(),
        }
    }

    pub(crate) const fn name(&self) -> &ParameterName {
        &self.name
    }

    pub(crate) const fn metadata(&self) -> &CteParameterBindingCollection {
        &self.metadata
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
    catalog: &ResolvedCteCatalog,
    dialect: Dialect,
) -> Result<RealizedCteQuery, CteRealizationError> {
    let bindings = OrderedBindings::from_catalog(catalog)?;
    let order = bindings.parameter_names();
    let mut statement = parse_piece(catalog.base_query(), dialect, &order, "base query")?;
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
#[path = "cte_realization/tests.rs"]
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
