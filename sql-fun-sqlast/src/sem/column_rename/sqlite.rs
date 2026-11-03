//! SQLite validates the rewritten namespaces before committing a schema rename.

use crate::{
    ast::Dialect,
    context::ParseContext,
    names::{ColumnName, FullName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, ColumnDefinition, ColumnRename, CreateTrigger,
        QueryProblem, SemScalarExpr, ViewLike,
        traversal::{ColumnRenamer, SemanticVisitorMut},
    },
};
use std::collections::HashMap;
impl ColumnRename {
    pub(super) fn sqlite_dependency_problems(
        &mut self,
        context: &impl ParseContext,
    ) -> Result<Vec<AnalysisProblem>, AnalysisError> {
        let originals: Vec<_> = context
            .relation_names()
            .into_iter()
            .filter_map(|name| {
                context
                    .get_view_impl(&name)
                    .map(|view| (name, view.clone()))
            })
            .collect();
        let mut outputs: HashMap<_, _> = originals
            .iter()
            .map(|(name, view)| (name.clone(), view.result_columns().names()))
            .collect();
        let mut problems = Vec::new();
        // Each pass propagates names through one more dependent view. Starting
        // each pass from the retained definitions preserves explicit SQL names.
        for _ in 0..=originals.len() {
            let mut validator = RenameNamespaces::new(context, self, outputs.clone(), true);
            for (name, original) in &originals {
                let mut view = original.clone();
                let checkpoint = validator.problems.len();
                validator.affected = false;
                view.rename_column_with(self, &mut validator)?;
                if !validator.affected {
                    validator.problems.truncate(checkpoint);
                }
                outputs.insert(name.clone(), view.result_columns().names());
            }
            problems = validator.problems;
            if validator.outputs == outputs {
                break;
            }
        }
        let mut validator = RenameNamespaces::new(context, self, outputs.clone(), true);
        for name in context.relation_names() {
            let triggers = if let Some(table) = context.get_table_impl(&name) {
                Some(table.triggers())
            } else if let Some(view) = context.get_view_impl(&name) {
                view.triggers()
            } else {
                None
            };
            if let Some(triggers) = triggers {
                for trigger in triggers.resolve(context, &name) {
                    let mut trigger = trigger.clone();
                    let checkpoint = validator.problems.len();
                    validator.affected = trigger.target() == self.table().full_name();
                    trigger.rename_column_with(self, &mut validator)?;
                    if !validator.affected {
                        validator.problems.truncate(checkpoint);
                    }
                }
            }
        }
        problems.extend(validator.problems);
        outputs.retain(|name, columns| {
            context
                .get_view_impl(name)
                .is_some_and(|view| columns != &view.result_columns().names())
        });
        self.sqlite_outputs = outputs;
        Ok(problems)
    }
    pub(crate) fn rewrite_sqlite_view(
        &self,
        context: &impl ParseContext,
        view: &mut ViewLike,
    ) -> Result<(), AnalysisError> {
        view.rename_column_with(
            self,
            &mut RenameNamespaces::new(context, self, self.sqlite_outputs.clone(), false),
        )
    }
    pub(crate) fn rewrite_sqlite_trigger(
        &self,
        context: &impl ParseContext,
        trigger: &mut CreateTrigger,
    ) -> Result<(), AnalysisError> {
        trigger.rename_column_with(
            self,
            &mut RenameNamespaces::new(context, self, self.sqlite_outputs.clone(), false),
        )
    }
}
struct RenameNamespaces<'a, C> {
    context: &'a C,
    rename: &'a ColumnRename,
    columns: ColumnRenamer<'a>,
    outputs: HashMap<FullName, Vec<ColumnName>>,
    scopes: Vec<Vec<NamespaceSource>>,
    joins: Vec<(usize, usize)>,
    natural_projection_changes: Vec<bool>,
    affected: bool,
    ctes: Vec<HashMap<String, SourceColumns>>,
    source_outputs: Vec<Vec<ColumnName>>,
    validate: bool,
    problems: Vec<AnalysisProblem>,
}
#[derive(Clone)]
struct SourceColumns {
    original: Vec<ColumnName>,
    current: Vec<ColumnName>,
}
struct NamespaceSource {
    alias: String,
    columns: Vec<ColumnName>,
    original: Vec<ColumnName>,
    unqualified: bool,
}
impl NamespaceSource {
    fn original_columns(sources: &[Self]) -> Vec<ColumnName> {
        sources
            .iter()
            .filter(|source| source.unqualified)
            .flat_map(|source| source.original.iter().cloned())
            .collect()
    }
    fn natural_pairs(left: &[ColumnName], right: &[ColumnName]) -> Vec<(usize, usize)> {
        left.iter()
            .enumerate()
            .flat_map(|(left_index, left)| {
                right
                    .iter()
                    .enumerate()
                    .filter_map(move |(right_index, right)| {
                        if left.as_str().eq_ignore_ascii_case(right.as_str()) {
                            Some((left_index, right_index))
                        } else {
                            None
                        }
                    })
            })
            .collect()
    }
    fn original_join(
        left: &[Self],
        right: &[Self],
        using: &[ColumnName],
        natural: bool,
    ) -> (Vec<ColumnName>, Vec<(usize, usize)>) {
        let mut columns = Self::original_columns(left);
        let right = Self::original_columns(right);
        let pairs = Self::natural_pairs(&columns, &right);
        let using = if natural {
            pairs
                .iter()
                .map(|(index, _)| columns[*index].clone())
                .collect()
        } else {
            using.to_vec()
        };
        columns.extend(right.into_iter().filter(|name| !using.contains(name)));
        (columns, pairs)
    }
    fn visible_columns(sources: &[Self]) -> Vec<ColumnName> {
        sources
            .iter()
            .filter(|source| source.unqualified)
            .flat_map(|source| source.columns.iter().cloned())
            .collect()
    }
}
impl<'a, C: ParseContext> RenameNamespaces<'a, C> {
    const fn new(
        context: &'a C,
        rename: &'a ColumnRename,
        outputs: HashMap<FullName, Vec<ColumnName>>,
        validate: bool,
    ) -> Self {
        Self {
            context,
            rename,
            columns: ColumnRenamer::new(rename),
            outputs,
            scopes: Vec::new(),
            joins: Vec::new(),
            natural_projection_changes: Vec::new(),
            affected: false,
            ctes: Vec::new(),
            problems: Vec::new(),
            source_outputs: Vec::new(),
            validate,
        }
    }
    fn install_join(
        &mut self,
        start: usize,
        original: Vec<ColumnName>,
        columns: Vec<ColumnName>,
        alias: Option<&AliasSpec>,
    ) {
        let Some(scope) = self.scopes.last_mut() else {
            return;
        };
        for source in &mut scope[start..] {
            source.unqualified = false;
        }
        if let Some(alias) = alias {
            scope.truncate(start);
            self.add_source(alias, &original, &columns);
        } else {
            scope.push(NamespaceSource {
                alias: String::new(),
                columns,
                original,
                unqualified: true,
            });
        }
    }
    fn add_source(&mut self, alias: &AliasSpec, original: &[ColumnName], columns: &[ColumnName]) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(NamespaceSource {
                alias: alias.name().as_str().to_owned(),
                columns: alias.visible_column_names(columns),
                original: alias.visible_column_names(original),
                unqualified: true,
            });
        }
    }
}
impl<C: ParseContext> SemanticVisitorMut for RenameNamespaces<'_, C> {
    fn source_outputs(&mut self, columns: &[ColumnName]) {
        self.source_outputs.push(columns.to_vec());
    }
    fn join_sensitive_star(&mut self) {
        if self.context.dialect() == Dialect::sqlite()
            && self.natural_projection_changes.last() == Some(&true)
        {
            todo!("SQLite NATURAL JOIN wildcard reconstruction (#868)");
        }
    }
    fn projection(
        &mut self,
        name: &mut Option<ColumnName>,
        explicit: bool,
        expanded: bool,
        value: &mut SemScalarExpr,
    ) -> Result<(), AnalysisError> {
        if self.context.dialect() == Dialect::sqlite()
            && !explicit
            && !expanded
            && value.namespace_reference().is_none()
        {
            let mut probe = OutputReferenceChanges {
                rename: self.rename,
                changed: false,
            };
            value.clone().walk(&mut probe)?;
            if probe.changed {
                todo!("SQLite implicit expression-derived output names (#867)");
            }
        }
        if !expanded {
            return Ok(());
        }
        let Some((alias, column, _)) = value.namespace_reference() else {
            return Ok(());
        };
        let destination = self.scopes.iter().rev().find_map(|scope| {
            scope.iter().find_map(|source| {
                if !source.alias.eq_ignore_ascii_case(alias) {
                    return None;
                }
                let index = source
                    .original
                    .iter()
                    .position(|original| original.as_str().eq_ignore_ascii_case(column.as_str()))?;
                source.columns.get(index).cloned()
            })
        });
        if let Some(destination) = destination {
            value.set_projection_name(&destination);
            if !explicit {
                *name = Some(destination);
            }
        }
        Ok(())
    }
    fn result_name(
        &mut self,
        name: &mut Option<ColumnName>,
        explicit: bool,
        value: &SemScalarExpr,
    ) {
        self.columns.result_name(name, explicit, value);
    }
    fn assignment_target(&mut self, value: &mut SemScalarExpr) {
        self.columns.assignment_target(value);
    }
    fn bound_definition(&mut self, column: &mut ColumnDefinition, destination: &SemScalarExpr) {
        self.columns.bound_definition(column, destination);
    }

    fn enter_join(&mut self) {
        let start = self.scopes.last().map_or(0, Vec::len);
        self.joins.push((start, start));
    }
    fn join_right(&mut self) {
        if let Some((_, split)) = self.joins.last_mut() {
            *split = self.scopes.last().map_or(0, Vec::len);
        }
    }
    fn leave_join(&mut self, using: &[ColumnName], natural: bool, alias: Option<&AliasSpec>) {
        let Some((start, split)) = self.joins.pop() else {
            return;
        };
        let Some(scope) = self.scopes.last_mut() else {
            return;
        };
        let (original, original_pairs) =
            NamespaceSource::original_join(&scope[start..split], &scope[split..], using, natural);
        let mut left = NamespaceSource::visible_columns(&scope[start..split]);
        let right = NamespaceSource::visible_columns(&scope[split..]);
        let current_pairs = NamespaceSource::natural_pairs(&left, &right);
        if natural
            && original_pairs != current_pairs
            && let Some(changed) = self.natural_projection_changes.last_mut()
        {
            *changed = true;
        }
        let using = if natural {
            left.iter()
                .filter(|name| right.contains(name))
                .cloned()
                .collect::<Vec<_>>()
        } else {
            using.to_vec()
        };
        for name in &using {
            if self.validate && (!left.contains(name) || !right.contains(name)) {
                self.problems
                    .push(AnalysisProblem::query(QueryProblem::reference_not_found(
                        name.as_str(),
                    )));
            }
        }
        left.extend(right.into_iter().filter(|name| !using.contains(name)));
        self.install_join(start, original, left, alias);
    }

    fn enter_query(&mut self) {
        self.natural_projection_changes.push(false);
        self.scopes.push(Vec::new());
        self.ctes.push(HashMap::new());
    }
    fn leave_query(&mut self) {
        self.natural_projection_changes.pop();
        self.scopes.pop();
        self.ctes.pop();
    }
    fn cte_columns(&mut self, name: &str, columns: &[ColumnName]) {
        if let Some(ctes) = self.ctes.last_mut() {
            ctes.insert(
                name.to_owned(),
                SourceColumns {
                    original: self
                        .source_outputs
                        .pop()
                        .unwrap_or_else(|| columns.to_vec()),
                    current: columns.to_vec(),
                },
            );
        }
    }
    fn cte_source(&mut self, name: &str, alias: &AliasSpec) {
        let columns = self
            .ctes
            .iter()
            .rev()
            .find_map(|ctes| ctes.get(name))
            .cloned();
        if let Some(columns) = columns {
            self.add_source(alias, &columns.original, &columns.current);
        }
    }
    fn source_binding(&mut self, relation: &FullName, alias: &AliasSpec) {
        self.affected |= relation == self.rename.table().full_name()
            || self.outputs.get(relation).is_some_and(|columns| {
                self.context
                    .get_view_impl(relation)
                    .is_some_and(|view| columns != &view.result_columns().names())
            });
        let original = if let Some(table) = self.context.get_table_impl(relation) {
            table.column_names()
        } else if let Some(view) = self.context.get_view_impl(relation) {
            view.result_columns().names()
        } else {
            Vec::new()
        };
        let mut columns = self
            .outputs
            .get(relation)
            .cloned()
            .unwrap_or_else(|| original.clone());
        if relation == self.rename.table().full_name() {
            self.rename.rename_names(&mut columns);
        }
        self.add_source(alias, &original, &columns);
    }
    fn derived_binding(&mut self, alias: &AliasSpec, columns: &[ColumnName]) {
        let original = self
            .source_outputs
            .pop()
            .unwrap_or_else(|| columns.to_vec());
        self.add_source(alias, &original, columns);
    }
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        self.columns.expression(expression)?;
        if !self.validate {
            return Ok(());
        }
        let Some((alias, column, unqualified)) = expression.namespace_reference() else {
            return Ok(());
        };
        // OLD/NEW and DML destinations are bound outside a SELECT namespace.
        if self.scopes.is_empty() {
            return Ok(());
        }
        let mut candidates = None;
        for scope in self.scopes.iter().rev() {
            let count = scope
                .iter()
                .filter(|source| {
                    if unqualified {
                        source.unqualified
                    } else {
                        source.alias.eq_ignore_ascii_case(alias)
                    }
                })
                .flat_map(|source| source.columns.iter())
                .filter(|name| name.as_str().eq_ignore_ascii_case(column.as_str()))
                .count();
            if count > 0 {
                candidates = Some(count);
                break;
            }
        }
        if candidates.is_some_and(|count| count > 1) {
            self.problems
                .push(AnalysisProblem::query(QueryProblem::ambiguous_reference(
                    column.as_str(),
                )));
        } else if candidates.is_none()
            && self.scopes.iter().any(|scope| {
                scope
                    .iter()
                    .any(|source| source.alias.eq_ignore_ascii_case(alias))
            })
        {
            self.problems
                .push(AnalysisProblem::query(QueryProblem::reference_not_found(
                    column.as_str(),
                )));
        }
        Ok(())
    }
}

struct OutputReferenceChanges<'a> {
    rename: &'a ColumnRename,
    changed: bool,
}
impl SemanticVisitorMut for OutputReferenceChanges<'_> {
    fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        let original = value.namespace_reference().map(|(_, name, _)| name.clone());
        value.rename_bound_column(self.rename);
        let current = value.namespace_reference().map(|(_, name, _)| name.clone());
        self.changed |= original != current;
        Ok(())
    }
}
