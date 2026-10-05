//! Checked CTE declaration and collection editing.

use crate::ast::validation::ContainsInvalidCteClause;
use crate::ast::{
    CommonTableExpression, Cte, CteBody, CteColumn, Dialect, Statement, StatementNode, With,
    WithClause,
};

/// A rejected common AST edit. The destination remains unchanged on failure.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct AstMutationError(MutationProblem);

#[derive(Debug, thiserror::Error)]
enum MutationProblem {
    #[error("this statement cannot carry a WITH clause")]
    UnsupportedCarrier,
    #[error("this statement kind is not permitted as a CTE body for {0:?}")]
    InvalidBody(Dialect),
    #[error("a CTE body cannot contain SELECT INTO or a data-modifying WITH clause")]
    NestedModification,
    #[error("CTE definitions must use the destination statement's dialect")]
    DifferentDialect,
    #[error("CTE identifier is not permitted by the selected dialect")]
    InvalidName,
    #[error("a CTE named {0:?} already exists in this WITH clause")]
    DuplicateName(String),
    #[error("CTE insertion index {0} exceeds the collection length")]
    InvalidIndex(usize),
    #[error("a CTE with SEARCH or CYCLE requires WITH RECURSIVE")]
    RequiresRecursive,
}

impl MutationProblem {
    const fn unsupported_carrier() -> AstMutationError {
        AstMutationError(Self::UnsupportedCarrier)
    }

    const fn invalid_body(dialect: Dialect) -> AstMutationError {
        AstMutationError(Self::InvalidBody(dialect))
    }

    const fn nested_modification() -> AstMutationError {
        AstMutationError(Self::NestedModification)
    }

    const fn different_dialect() -> AstMutationError {
        AstMutationError(Self::DifferentDialect)
    }

    const fn invalid_name() -> AstMutationError {
        AstMutationError(Self::InvalidName)
    }

    fn duplicate_name(name: &str) -> AstMutationError {
        AstMutationError(Self::DuplicateName(name.to_owned()))
    }

    const fn invalid_index(index: usize) -> AstMutationError {
        AstMutationError(Self::InvalidIndex(index))
    }

    const fn requires_recursive() -> AstMutationError {
        AstMutationError(Self::RequiresRecursive)
    }
}

/// An owned CTE declaration with dialect provenance and an immutable body.
///
/// The name is the logical identifier spelling, without SQL quote delimiters.
/// A definition can only be inserted into a statement of the same dialect.
#[derive(Clone, Debug)]
pub struct CteDefinition {
    node: Cte,
    dialect: Dialect,
}

impl CteDefinition {
    /// Construct a CTE from a query or a dialect-permitted modifying statement.
    ///
    /// The declaration initially has no column aliases or materialization hint.
    /// The body is copied intact, including its existing WITH clause. This checks
    /// grammar placement, not catalog bindings or complete semantic validity.
    /// Names cannot contain NUL; PostgreSQL also rejects empty names. Whitespace
    /// is preserved without trimming, and SQL quote delimiters must be omitted.
    pub fn new(name: &str, body: &Statement) -> Result<Self, AstMutationError> {
        Self::validate_name(name, body.dialect)?;
        let node = Cte {
            name: name.to_owned(),
            columns: Vec::new(),
            materialized: None,
            body: body.node.cte_body(body.dialect)?,
            search: None,
            cycle: None,
        };
        let definition = Self {
            node,
            dialect: body.dialect,
        };
        definition.validate()?;
        Ok(definition)
    }

    /// Logical identifier spelling, without quote delimiters.
    pub fn name(&self) -> &str {
        &self.node.name
    }

    /// Dialect selected when the body was parsed.
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// Set the explicitly supplied column aliases in order.
    ///
    /// This does not infer result columns or require a catalog. An invalid name
    /// leaves the declaration unchanged.
    /// Aliases cannot contain NUL; PostgreSQL also rejects empty aliases.
    /// Whitespace is preserved, and SQL quote delimiters must be omitted.
    pub fn set_columns(&mut self, columns: &[&str]) -> Result<(), AstMutationError> {
        let mut names = Vec::new();
        for column in columns {
            Self::validate_name(column, self.dialect)?;
            names.push((*column).to_owned());
        }
        self.node.columns = CteColumn::from_names(&names);
        Ok(())
    }

    /// Set MATERIALIZED, NOT MATERIALIZED, or remove the explicit hint with None.
    pub const fn set_materialized(&mut self, materialized: Option<bool>) {
        self.node.materialized = materialized;
    }

    /// Inspect the complete declaration before insertion.
    pub const fn view(&self) -> CommonTableExpression<'_> {
        CommonTableExpression {
            node: &self.node,
            dialect: self.dialect,
        }
    }

    fn validate_name(name: &str, dialect: Dialect) -> Result<(), AstMutationError> {
        if (name.is_empty() && dialect == Dialect::postgres()) || name.contains('\0') {
            Err(MutationProblem::invalid_name())
        } else {
            Ok(())
        }
    }

    fn validate(&self) -> Result<(), AstMutationError> {
        Self::validate_name(&self.node.name, self.dialect)?;
        self.node.body.validate_cte_body(self.dialect)?;
        if self.node.contains_invalid_cte_clause() {
            return Err(MutationProblem::nested_modification());
        }
        Ok(())
    }
}

impl CommonTableExpression<'_> {
    /// Copy this declaration, preserving its body and all declaration attributes.
    ///
    /// Insertion rechecks dialect and placement constraints, including when the
    /// source was parsed without semantic analysis.
    pub fn to_owned(&self) -> CteDefinition {
        CteDefinition {
            node: self.node.clone(),
            dialect: self.dialect,
        }
    }
}

impl StatementNode {
    fn cte_body(&self, dialect: Dialect) -> Result<CteBody, AstMutationError> {
        match self {
            Self::Query(node) => Ok(CteBody::query(node.clone())),
            Self::Insert(node) => Ok(CteBody::insert(node.clone())),
            Self::Update(node) => Ok(CteBody::update(node.clone())),
            Self::Delete(node) => Ok(CteBody::delete(node.clone())),
            _ => Err(MutationProblem::invalid_body(dialect)),
        }
    }

    const fn with_node_mut(&mut self) -> Option<&mut With> {
        match self {
            Self::Query(node) => Some(&mut node.with),
            Self::Insert(node) => Some(&mut node.with),
            Self::Update(node) => Some(&mut node.with),
            Self::Delete(node) => Some(&mut node.with),
            _ => None,
        }
    }
}

impl CteBody {
    fn validate_cte_body(&self, dialect: Dialect) -> Result<(), AstMutationError> {
        if dialect == Dialect::sqlite() && self.is_data_modifying() {
            return Err(MutationProblem::invalid_body(dialect));
        }
        Ok(())
    }

    pub(super) const fn is_data_modifying(&self) -> bool {
        match self {
            Self::Query(_) => false,
            Self::Insert(_) | Self::Update(_) | Self::Delete(_) | Self::Merge(_) => true,
        }
    }
}

impl Cte {
    const fn requires_recursive(&self) -> bool {
        self.search.is_some() || self.cycle.is_some()
    }
}

impl With {
    fn requires_recursive(&self) -> bool {
        self.entries.iter().any(Cte::requires_recursive)
    }
}

/// Access and explicitly create the WITH clause of a capable statement.
///
/// The owned statement dispatches to its query or DML carrier. Other statement
/// kinds return no clause and reject creation. Editors expose checked operations
/// instead of references that allow replacing nodes without validation.
pub trait HaveWithClause {
    /// Inspect an existing nonempty WITH clause.
    fn with_clause(&self) -> Option<WithClause<'_>>;
    /// Edit an existing nonempty WITH clause.
    fn with_clause_mut(&mut self) -> Option<WithClauseMut<'_>>;
    /// Obtain a WITH editor, creating an initially empty, nonrecursive clause.
    ///
    /// Existing clauses keep their recursive flag. An empty clause is not
    /// observable as SQL WITH syntax until a CTE is successfully inserted.
    fn ensure_with_clause(&mut self) -> Result<WithClauseMut<'_>, AstMutationError>;
}

impl HaveWithClause for Statement {
    fn with_clause(&self) -> Option<WithClause<'_>> {
        Self::with_clause(self)
    }

    fn with_clause_mut(&mut self) -> Option<WithClauseMut<'_>> {
        let node = self.node.with_node_mut()?;
        if node.entries.is_empty() {
            None
        } else {
            Some(WithClauseMut {
                node,
                dialect: self.dialect,
            })
        }
    }

    fn ensure_with_clause(&mut self) -> Result<WithClauseMut<'_>, AstMutationError> {
        let node = self
            .node
            .with_node_mut()
            .ok_or_else(MutationProblem::unsupported_carrier)?;
        Ok(WithClauseMut {
            node,
            dialect: self.dialect,
        })
    }
}

/// A checked editor bound to one statement's dialect and WITH collection.
pub struct WithClauseMut<'a> {
    node: &'a mut With,
    dialect: Dialect,
}

impl WithClauseMut<'_> {
    /// Insert at the explicit zero-based position, preserving surrounding order.
    ///
    /// Use the current length to append. Duplicate names, invalid positions,
    /// different source dialects, and invalid CTE placement leave the clause
    /// unchanged. This performs no catalog lookup or dependency reordering.
    pub fn insert(
        &mut self,
        index: usize,
        definition: &CteDefinition,
    ) -> Result<(), AstMutationError> {
        if definition.dialect != self.dialect {
            return Err(MutationProblem::different_dialect());
        }
        if index > self.node.entries.len() {
            return Err(MutationProblem::invalid_index(index));
        }
        if self.view().find(definition.name()).is_some() {
            return Err(MutationProblem::duplicate_name(definition.name()));
        }
        if definition.node.requires_recursive() && !self.node.recursive {
            return Err(MutationProblem::requires_recursive());
        }
        definition.validate()?;
        self.node.entries.insert(index, definition.node.clone());
        Ok(())
    }

    /// Explicitly set whether this clause uses WITH RECURSIVE.
    ///
    /// This flag does not resolve self-references or prove recursive semantics.
    /// Disabling it is rejected while any declaration uses SEARCH or CYCLE.
    pub fn set_recursive(&mut self, recursive: bool) -> Result<(), AstMutationError> {
        if !recursive && self.node.requires_recursive() {
            return Err(MutationProblem::requires_recursive());
        }
        self.node.recursive = recursive;
        Ok(())
    }

    /// Inspect the collection, including an editor's not-yet-populated clause.
    pub const fn view(&self) -> WithClause<'_> {
        WithClause {
            node: self.node,
            dialect: self.dialect,
        }
    }
}
