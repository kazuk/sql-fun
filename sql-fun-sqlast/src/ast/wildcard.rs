//! Location-checked wildcard projection replacement in the owning common AST.

mod rows_source;
mod source;
use self::source::SourcePlans;

use crate::ast::{
    Access, Assignment, BooleanExpr, Call, Case, CaseArm, Cast, Collate, Conflict, Cte, CteBody,
    Cycle, Delete, DerivedTable, Expr, Frame, FrameBound, FromGroup, FromItem, Grouping, InTable,
    IndexExpression, Indirection, Insert, Into, Join, Name, NamedArgument, NullTest, OperatorExpr,
    Order, Query, QueryBody, Raise, Select, SetQuery, Slice, Subquery, SyntaxOption, SyntaxValue,
    TableFunction, TableFunctions, TableSample, Target, TruthTest, TypeName, TypedColumn, Update,
    Window, With,
};

use crate::ast::{Statement, StatementNode};
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
        let mut locations = Vec::new();
        reparsed.node.visit_targets(&mut |targets, returning, _| {
            locations.push((
                returning,
                targets
                    .iter()
                    .map(|target| target.location)
                    .collect::<Vec<_>>(),
            ));
        });
        if locations
            .iter()
            .any(|(_, targets)| targets.iter().any(Option::is_none))
        {
            return Err(WildcardRewriteError::missing_location());
        }
        let mut unique_locations = std::collections::HashSet::new();
        if locations.iter().any(|(_, targets)| {
            targets
                .iter()
                .flatten()
                .any(|location| !unique_locations.insert(*location))
        }) {
            return Err(WildcardRewriteError::ambiguous());
        }
        let mut index = 0;
        let mut compatible = true;
        self.node.visit_targets(&mut |targets, returning, _| {
            compatible &= locations
                .get(index)
                .is_some_and(|(source_returning, source_targets)| {
                    *source_returning == returning && source_targets.len() == targets.len()
                });
            index += 1;
        });
        if !compatible || index != locations.len() {
            return Err(WildcardRewriteError::invalid(
                "target collections differ after the parser round trip",
            ));
        }
        let mut locations = locations.into_iter();
        self.node.visit_targets(&mut |targets, _, _| {
            let (_, source) = locations.next().expect("validated target collection");
            for (target, location) in targets.iter_mut().zip(source) {
                target.location = location;
            }
        });
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
        if expansions
            .iter()
            .any(|expansion| expansion.is_table_dependent() && expansion.location().is_none())
        {
            return Err(WildcardRewriteError::missing_location());
        }
        let sources = SourcePlans::prepare(self, expansions)?;
        let mut edits = Vec::new();
        for expansion in expansions.iter().filter(|value| value.is_table_dependent()) {
            let location = expansion
                .location()
                .ok_or_else(|| WildcardRewriteError::missing_location())?;
            if edits.iter().any(|(previous, _)| *previous == location) {
                return Err(WildcardRewriteError::duplicate_location());
            }
            let mut targets = Vec::new();
            for (index, column) in expansion.columns().iter().enumerate() {
                let expression = if let Some(record) = expansion.typed_record_at(index) {
                    self::source::typed_record_reference(record)
                } else if let Some((alias, ordinal)) = expansion.source_ordinal_at(index) {
                    self::source::ordinal_reference(alias, ordinal)
                } else if let Some(parts) = expansion.reference_at(index) {
                    Expr::projection_column(crate::ast::Name::from_parts(parts))
                } else {
                    column
                        .expression()
                        .ok_or_else(|| {
                            WildcardRewriteError::invalid(
                                "wildcard result has no resolved expression",
                            )
                        })?
                        .projection_expression(expansion.qualifiers())?
                };
                let alias = column.column_name().as_ref().ok_or_else(|| {
                    WildcardRewriteError::invalid("wildcard result has no output name")
                })?;
                targets.push(Target {
                    expression,
                    alias: Some(alias.to_string()),
                    location: None,
                });
            }
            edits.push((location, targets));
        }
        for (location, _) in &edits {
            let mut count = 0;
            let mut wildcard = false;
            self.node
                .visit_targets(&mut |targets, _returning, _sources| {
                    for target in targets {
                        if target.location == Some(*location) {
                            count += 1;
                            wildcard = target.expression.is_projection_star();
                        }
                    }
                });
            if count != 1 {
                return Err(if count == 0 {
                    WildcardRewriteError::not_found()
                } else {
                    WildcardRewriteError::ambiguous()
                });
            }
            if !wildcard {
                return Err(WildcardRewriteError::wrong_node_kind());
            }
        }
        let mut empty_returning = false;
        self.node
            .visit_targets(&mut |targets, returning, _sources| {
                if returning && !targets.is_empty() {
                    let mut width = targets.len();
                    for target in targets {
                        if let Some((_, replacement)) = edits
                            .iter()
                            .find(|(location, _)| target.location == Some(*location))
                        {
                            width = width - 1 + replacement.len();
                        }
                    }
                    empty_returning |= width == 0;
                }
            });
        if empty_returning {
            return Err(WildcardRewriteError::empty_returning());
        }
        let count = edits.len();
        self.node
            .visit_targets(&mut |targets, _returning, source_items| {
                sources.apply(targets, source_items);
                for index in (0..targets.len()).rev() {
                    if let Some(edit) = edits
                        .iter_mut()
                        .find(|(location, _)| targets[index].location == Some(*location))
                    {
                        targets.splice(index..=index, std::mem::take(&mut edit.1));
                    }
                }
            });
        Ok(count)
    }
}

#[cfg(test)]
mod tests;

impl Expr {
    fn is_projection_star(&self) -> bool {
        matches!(self, Self::Star(_))
    }
}

/// Visits each projection collection, including query-bearing expression edges.
pub(super) trait VisitTargets {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem]));
}
impl<T: VisitTargets> VisitTargets for Vec<T> {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        for item in self {
            item.visit_targets(visitor);
        }
    }
}
impl<T: VisitTargets> VisitTargets for Option<T> {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        if let Some(item) = self {
            item.visit_targets(visitor);
        }
    }
}
impl<T: VisitTargets> VisitTargets for Box<T> {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        self.as_mut().visit_targets(visitor);
    }
}
macro_rules! walk_fields {
    ($($type:ty => $($field:ident),+;)+) => {
        $(impl VisitTargets for $type {
            fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
                crate::sem::with_stack(|| { $(self.$field.visit_targets(visitor);)+ });
            }
        })+
    };
}
walk_fields! {

    Query => with, body, order, limit, offset;
    SetQuery => left, right;
    Target => expression;
    Order => expression;
    Cte => body, cycle;
    Cycle => value, default;
    TypeName => modifiers;
    TypedColumn => type_name;
    DerivedTable => query;
    FromGroup => items;
    Join => left, right, condition;
    TableFunctions => calls, columns;
    TableFunction => expression, columns;
    TableSample => relation, arguments, repeatable;
    Window => partition, order, frame;
    Frame => start, end;
    Assignment => indirection, value;
    Conflict => inference, inference_predicate, assignments, predicate, next;
    IndexExpression => expression, options;
    SyntaxOption => value;
    InTable => argument, arguments;
    OperatorExpr => left, right, escape;
    BooleanExpr => arguments;
    NullTest => argument;
    TruthTest => argument;
    Cast => argument, target;
    Collate => argument;
    Call => arguments, within_order, order, filter, over;
    Case => argument, arms, otherwise;
    CaseArm => condition, result;
    Subquery => query, test;
    Indirection => argument, items;
    Slice => lower, upper;
    NamedArgument => argument;
    Raise => message;
    With => entries;
}

impl VisitTargets for Select {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        self.into.visit_targets(visitor);
        self.distinct.visit_targets(visitor);
        self.targets.visit_targets(visitor);
        visitor(&mut self.targets, false, &mut self.from);
        self.from.visit_targets(visitor);
        self.predicate.visit_targets(visitor);
        self.group.visit_targets(visitor);
        self.having.visit_targets(visitor);
        self.windows.visit_targets(visitor);
    }
}
impl VisitTargets for Insert {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        self.with.visit_targets(visitor);
        self.columns.visit_targets(visitor);
        self.source.visit_targets(visitor);
        self.conflict.visit_targets(visitor);
        self.returning.visit_targets(visitor);
        visitor(&mut self.returning, true, &mut []);
    }
}
impl VisitTargets for Update {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        self.with.visit_targets(visitor);
        self.assignments.visit_targets(visitor);
        self.predicate.visit_targets(visitor);
        self.returning.visit_targets(visitor);
        visitor(&mut self.returning, true, &mut self.from);
        self.from.visit_targets(visitor);
    }
}
impl VisitTargets for Delete {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        self.with.visit_targets(visitor);
        self.predicate.visit_targets(visitor);
        self.returning.visit_targets(visitor);
        visitor(&mut self.returning, true, &mut self.using);
        self.using.visit_targets(visitor);
    }
}
impl VisitTargets for StatementNode {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        match self {
            Self::Query(node) => node.visit_targets(visitor),
            Self::Insert(node) => node.visit_targets(visitor),
            Self::Update(node) => node.visit_targets(visitor),
            Self::Delete(node) => node.visit_targets(visitor),
            Self::Merge(node) => node.visit_targets(visitor),
            _ => {}
        }
    }
}

impl VisitTargets for CteBody {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        match self {
            Self::Query(node) => node.visit_targets(visitor),
            Self::Insert(node) => node.visit_targets(visitor),
            Self::Update(node) => node.visit_targets(visitor),
            Self::Delete(node) => node.visit_targets(visitor),
            Self::Merge(node) => node.visit_targets(visitor),
        }
    }
}

impl VisitTargets for QueryBody {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        match self {
            Self::Select(node) => node.visit_targets(visitor),
            Self::Values(rows) => rows.visit_targets(visitor),
            Self::Set(node) => node.visit_targets(visitor),
        }
    }
}

impl VisitTargets for Expr {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        crate::sem::with_stack(|| match self {
            Self::Literal(_)
            | Self::Column(_)
            | Self::Star(_)
            | Self::Parameter(_)
            | Self::SqlValue(_)
            | Self::Default
            | Self::CurrentOf(_)
            | Self::MergeAction => {}
            Self::Operator(node) => node.visit_targets(visitor),
            Self::Boolean(node) => node.visit_targets(visitor),
            Self::NullTest(node) => node.visit_targets(visitor),
            Self::TruthTest(node) => node.visit_targets(visitor),
            Self::Cast(node) => node.visit_targets(visitor),
            Self::Collate(node) => node.visit_targets(visitor),
            Self::Call(node) => node.visit_targets(visitor),
            Self::Case(node) => node.visit_targets(visitor),
            Self::Subquery(node) => node.visit_targets(visitor),
            Self::Array(nodes)
            | Self::Row(nodes)
            | Self::List(nodes)
            | Self::Parenthesized(nodes)
            | Self::Grouping(nodes) => nodes.visit_targets(visitor),
            Self::Indirection(node) => node.visit_targets(visitor),
            Self::NamedArgument(node) => node.visit_targets(visitor),
            Self::Raise(node) => node.visit_targets(visitor),
            Self::InTable(node) => node.visit_targets(visitor),
            Self::Document(node) => node.visit_targets(visitor),
        })
    }
}

impl VisitTargets for FromItem {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        crate::sem::with_stack(|| match self {
            Self::Relation(_) => {}
            Self::Query(node) => node.visit_targets(visitor),
            Self::Join(node) => node.visit_targets(visitor),
            Self::Functions(node) => node.visit_targets(visitor),
            Self::Group(node) => node.visit_targets(visitor),
            Self::Sample(node) => node.visit_targets(visitor),
            Self::Xml(node) => node.visit_targets(visitor),
            Self::Json(node) => node.visit_targets(visitor),
        })
    }
}

impl VisitTargets for Grouping {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        crate::sem::with_stack(|| match self {
            Self::Expression(node) => node.visit_targets(visitor),
            Self::Empty => {}
            Self::Rollup(nodes) | Self::Cube(nodes) | Self::Sets(nodes) | Self::Simple(nodes) => {
                nodes.visit_targets(visitor)
            }
        })
    }
}

impl VisitTargets for Access {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        match self {
            Self::Field(_) | Self::Star => {}
            Self::Index(node) => node.visit_targets(visitor),
            Self::Slice(node) => node.visit_targets(visitor),
        }
    }
}

impl VisitTargets for FrameBound {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        match self {
            Self::Preceding(node) | Self::Following(node) => node.visit_targets(visitor),
            Self::UnboundedPreceding | Self::CurrentRow | Self::UnboundedFollowing => {}
        }
    }
}

impl VisitTargets for SyntaxValue {
    fn visit_targets(&mut self, visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem])) {
        match self {
            Self::Expression(node) => node.visit_targets(visitor),
            Self::Type(node) => node.visit_targets(visitor),
        }
    }
}

impl VisitTargets for Into {
    fn visit_targets(
        &mut self,
        _visitor: &mut impl FnMut(&mut Vec<Target>, bool, &mut [FromItem]),
    ) {
    }
}

impl SemScalarExpr {
    fn projection_expression(
        &self,
        qualifiers: &[crate::sem::WildcardQualifier],
    ) -> Result<Expr, WildcardRewriteError> {
        crate::sem::with_stack(|| match self {
            Self::ColumnRef(reference) => {
                if let Some(table) = self.table_column_reference() {
                    if let Some(qualifier) = qualifiers.iter().find(|qualifier| {
                        qualifier.relation() == table.relation()
                            && qualifier.alias() == table.alias()
                    }) {
                        let mut parts = qualifier.parts().to_vec();
                        parts.push(reference.get_column_name().to_string());
                        return Ok(Expr::projection_column(crate::ast::Name::from_parts(
                            &parts,
                        )));
                    }
                }
                let (alias, column) = reference.projection_reference().ok_or_else(|| {
                    WildcardRewriteError::invalid("wildcard column binding is unresolved")
                })?;
                let parts = if alias.is_empty() {
                    vec![column.to_string()]
                } else {
                    vec![alias.to_owned(), column.to_string()]
                };
                Ok(Expr::projection_column(crate::ast::Name::from_parts(
                    &parts,
                )))
            }
            Self::Coalesce(value) => {
                let arguments = value
                    .arguments()
                    .iter()
                    .map(|argument| argument.projection_expression(qualifiers))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Expr::projection_call(crate::ast::Call {
                    sql_syntax: false,
                    kind: crate::ast::CallKind::special("coalesce"),
                    name: crate::ast::Name::local("coalesce"),
                    argument_locations: vec![None; arguments.len()],
                    arguments,
                    star: false,
                    distinct: false,
                    variadic: false,
                    within_group: false,
                    within_order: Vec::new(),
                    order: Vec::new(),
                    filter: None,
                    over: None,
                    location: None,
                }))
            }
            Self::ImplicitCast(value) => {
                let (argument, target) = value.projection_operand();
                argument.projection_cast(target, qualifiers)
            }
            Self::TypeCast(value) => {
                let (argument, target) = value.projection_operand();
                argument.projection_cast(target, qualifiers)
            }
            _ => Err(WildcardRewriteError::invalid(
                "resolved wildcard expression has no executable projection representation",
            )),
        })
    }

    fn projection_cast(
        &self,
        target: &crate::sem::TypeReference,
        qualifiers: &[crate::sem::WildcardQualifier],
    ) -> Result<Expr, WildcardRewriteError> {
        if target.type_definition().is_some() {
            return Err(WildcardRewriteError::invalid(
                "wildcard cast target has no SQL type name",
            ));
        }
        let name = target.full_name();
        let mut parts = Vec::new();
        if let Some(catalog) = name.catalog() {
            parts.push(catalog.to_string());
        }
        if let Some(schema) = name.schema() {
            parts.push(schema.to_string());
        }
        parts.push(name.local_name());
        Ok(Expr::projection_cast_node(crate::ast::Cast {
            argument: self.projection_expression(qualifiers)?,
            target: TypeName {
                name: crate::ast::Name::from_parts(&parts),
                modifiers: Vec::new(),
                array_bounds: if target.is_array() == Some(true) {
                    vec![None]
                } else {
                    Vec::new()
                },
                set_of: false,
                percent_type: false,
                sqlite_name_snapshot: None,
                sqlite_parts: Vec::new(),
                sqlite_modifiers: None,
                location: None,
            },
        }))
    }
}

impl Expr {
    fn projection_function_alias(&self) -> Option<&str> {
        match self {
            Self::Call(call) => call.name.projection_local_name(),
            _ => None,
        }
    }
    fn projection_column(name: Name) -> Self {
        Self::Column(name)
    }
    fn projection_call(call: Call) -> Self {
        Self::Call(Box::new(call))
    }
    fn projection_cast_node(cast: Cast) -> Self {
        Self::Cast(Box::new(cast))
    }
    fn projection_source_row(alias: &str) -> Self {
        Self::Row(vec![Self::Star(Name::local(alias))])
    }
    fn projection_field(argument: Self, field: String) -> Self {
        Self::Indirection(Box::new(Indirection {
            argument,
            items: vec![Access::projection_field(field)],
        }))
    }
}
impl Access {
    fn projection_field(field: String) -> Self {
        Self::Field(field)
    }
}
impl QueryBody {
    fn projection_select(select: Select) -> Self {
        Self::Select(Box::new(select))
    }
}

impl Name {
    fn projection_local_name(&self) -> Option<&str> {
        self.0.last().map(String::as_str)
    }
}
