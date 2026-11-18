use std::collections::{HashMap, HashSet};

use crate::{
    ast::{
        Cte, CteBody, Dialect, With, recursive_cte::cte_key, validation::ContainsInvalidCteClause,
    },
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, CreateRule as SemCreateRule, CteStatement, DeleteStatement,
        EngineFeatureValidation, FromClause, InsertStatement, MergeStatement, QueryCte,
        ResolutionFailure, RuleIdentity, ScalarAnalysisContext, SelectClauses, SelectStatement,
        UpdateStatement, WithClause,
        problem::{QueryProblem, RuleProblem},
    },
    stack::with_stack,
};

impl CteStatement {
    pub(crate) fn ast_require_data_modifying_placement(
        ast: &CteBody,
        context: &mut impl ParseContext,
        top_level: bool,
    ) -> Result<(), AnalysisError> {
        if (matches!(
            ast,
            CteBody::Insert(_) | CteBody::Update(_) | CteBody::Delete(_) | CteBody::Merge(_)
        ) && !top_level)
            || ast.contains_invalid_cte_clause()
        {
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::invalid_cte_placement(),
            ))?;
        }
        Ok(())
    }
}

impl WithClause {
    pub(crate) fn ast_local_definitions(ast: &With, mut visible: Self) -> Vec<QueryCte> {
        let mut local = Vec::new();
        for entry in ast.entries() {
            if let Some(statement) = visible.take(entry.name()) {
                let mut cte = QueryCte::new(
                    entry.name(),
                    entry.column_names(),
                    entry.materialized(),
                    statement,
                );
                if ast.recursive() {
                    cte.set_resolution_order(visible.resolution_order(entry.name()));
                }
                local.push(cte);
            }
        }
        local
    }
}

#[cfg(test)]
mod test_ctes_are_owned_once_in_declaration_order {
    use crate::{
        ast::Dialect,
        sem::{AnalysisError, PgBuiltInType, SemScalarExpr},
        test_helpers::query::{output_type, resolve},
    };

    #[test]
    fn ctes_are_owned_once_in_declaration_order() {
        let (mut query, context) = resolve(
            "WITH a AS (SELECT $1 + 1 AS n), b AS (SELECT n FROM a) SELECT n FROM b ORDER BY n",
            Dialect::postgres(),
            true,
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        let [first, second] = query.query_clauses().unwrap().with() else {
            panic!("two local CTE declarations");
        };
        assert_eq!(first.name(), "a");
        assert_eq!(second.name(), "b");
        struct Parameters(usize);
        impl crate::sem::traversal::SemanticVisitorMut for Parameters {
            fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                if matches!(expression, SemScalarExpr::Param(_)) {
                    self.0 += 1;
                }
                Ok(())
            }
        }
        let mut parameters = Parameters(0);
        query.walk(&mut parameters).unwrap();
        assert_eq!(parameters.0, 1);
        assert_eq!(output_type(&query, "n"), Some(PgBuiltInType::int4()));
    }
}

impl WithClause {
    pub(crate) fn from_ast(
        ast: &With,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        let mut with = outer
            .map(ScalarAnalysisContext::inherited_ctes)
            .unwrap_or_default();
        with.clear_resolution_order();
        let dialect = context.dialect();
        let ordered = if ast.recursive() {
            match RecursiveWith::order(ast, dialect) {
                Some(entries) => entries,
                None => {
                    context.report_analysis_problem(AnalysisProblem::query(
                        QueryProblem::invalid_recursive_cte(
                            "WITH RECURSIVE contains duplicate names or mutual recursion",
                        ),
                    ))?;
                    for entry in ast.entries() {
                        with.insert_resolved(context, entry.name(), CteStatement::unresolved());
                    }
                    return Ok(with);
                }
            }
        } else {
            ast.entries().iter().map(|entry| (entry, false)).collect()
        };
        for (entry, recursive) in ordered {
            let body = CteStatement::from_ast_entry(entry, recursive, context, &with, outer)?;
            with.insert_resolved(context, entry.name(), body);
            if ast.recursive() {
                with.record_resolution(entry.name());
            }
        }
        Ok(with)
    }
}

impl CteStatement {
    fn from_ast_entry(
        entry: &Cte,
        recursive: bool,
        context: &mut impl ParseContext,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        if entry.requires_recursive() {
            return Err(AnalysisError::unexpected_input_error(
                "SEARCH and CYCLE are not supported in recursive CTE analysis",
            ));
        }
        let rule = match outer {
            Some(scope) => scope.direct_rule_identity(),
            None => None,
        };
        if let Some(identity) = rule
            && matches!(entry.name().as_str(), "old" | "new")
        {
            context.report_analysis_problem(AnalysisProblem::rule(&RuleProblem::definition(
                identity,
                "CTE conflicts with rule OLD/NEW namespace",
            )))?;
        }
        Self::ast_require_data_modifying_placement(
            entry.body(),
            context,
            outer.is_none() || rule.is_some(),
        )?;
        let from = FromClause::default();
        let restricted = outer.map(ScalarAnalysisContext::without_rule_rows);
        let scope = ScalarAnalysisContext::nested(with, &from, restricted.as_ref());
        if recursive {
            Self::from_recursive_ast(entry, context, &scope)
        } else {
            Self::from_ast_cte_body(entry.body(), context, &scope, &entry.column_names())
        }
    }

    pub(crate) fn from_ast_cte_body(
        ast: &CteBody,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &[String],
    ) -> Result<Self, AnalysisError> {
        if !EngineFeatureValidation::cte_body(ast, context)? {
            return Ok(Self::unresolved());
        }
        match ast {
            CteBody::Query(query) => {
                SelectStatement::from_ast_cte_body(context, scope, columns, query)
            }
            CteBody::Insert(insert) => {
                InsertStatement::from_ast_cte_body(context, scope, columns, insert)
            }
            CteBody::Update(update) => {
                UpdateStatement::from_ast_cte_body(context, scope, columns, update)
            }
            CteBody::Delete(delete) => {
                DeleteStatement::from_ast_cte_body(context, scope, columns, delete)
            }
            CteBody::Merge(merge) => {
                MergeStatement::from_ast_cte_body(context, scope, columns, merge)
            }
        }
    }
}

impl CteStatement {
    pub(crate) fn validate_ast_select_rule_ctes(
        ast: &CteBody,
        identity: &RuleIdentity,
    ) -> Result<(), ResolutionFailure> {
        match ast {
            CteBody::Query(query) => {
                SelectStatement::validate_ast_select_rule_ctes(query.as_ref(), identity)
            }
            _ => SemCreateRule::ast_invalid(
                identity,
                "ON SELECT rule cannot contain data-modifying CTEs",
            ),
        }
    }
}

struct RecursiveWith<'a> {
    entries: HashMap<String, &'a Cte>,
    dependencies: HashMap<String, HashMap<String, Vec<bool>>>,
    visiting: HashSet<String>,
    finished: HashSet<String>,
    order: Vec<(&'a Cte, bool)>,
}

impl<'a> RecursiveWith<'a> {
    fn order(ast: &'a With, dialect: Dialect) -> Option<Vec<(&'a Cte, bool)>> {
        let mut graph = Self {
            entries: HashMap::new(),
            dependencies: HashMap::new(),
            visiting: HashSet::new(),
            finished: HashSet::new(),
            order: Vec::new(),
        };
        let mut names = Vec::new();
        for entry in ast.entries() {
            let name = cte_key(entry.name(), dialect);
            if graph.entries.insert(name.clone(), entry).is_some() {
                return None;
            }
            graph
                .dependencies
                .insert(name.clone(), entry.body().cte_references(dialect));
            names.push(name);
        }
        for name in &names {
            graph.visit(name, &names)?;
        }
        Some(graph.order)
    }

    fn visit(&mut self, name: &str, names: &[String]) -> Option<()> {
        if self.finished.contains(name) {
            return Some(());
        }
        if !self.visiting.insert(name.to_owned()) {
            return None;
        }
        for dependency in names {
            if dependency != name && self.dependencies[name].contains_key(dependency) {
                with_stack(|| self.visit(dependency, names))?;
            }
        }
        self.visiting.remove(name);
        self.finished.insert(name.to_owned());
        self.order.push((
            self.entries[name],
            self.dependencies[name].contains_key(name),
        ));
        Some(())
    }
}

impl CteStatement {
    fn from_recursive_ast(
        entry: &Cte,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let dialect = context.dialect();
        let Some((query, set)) = entry.body().recursive_union(entry.name(), dialect) else {
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::invalid_recursive_cte(&format!(
                    "invalid recursive reference or UNION shape for CTE {}",
                    entry.name()
                )),
            ))?;
            return Ok(Self::unresolved());
        };
        let aliases = entry.column_names();
        let mut query = SelectStatement::analyze_ast_query_with(
            query,
            context,
            Some(scope),
            |context, with, outer, _| {
                let from = FromClause::default();
                let scope = ScalarAnalysisContext::nested(with, &from, outer);
                let anchor = SelectStatement::from_ast_in(set.left(), context, Some(&scope))?;
                if aliases.len() > anchor.result_columns().len() {
                    context.report_analysis_problem(AnalysisProblem::query(
                        QueryProblem::invalid_recursive_cte(&format!(
                            "CTE {} has more aliases than output columns",
                            entry.name()
                        )),
                    ))?;
                }
                let columns = anchor.result_columns().recursive_binding(dialect, &aliases);
                let binding = SelectStatement::ast_build_result(
                    columns,
                    FromClause::default(),
                    None,
                    SelectClauses::default(),
                );
                let recursive_with =
                    with.with_recursive_anchor(context, entry.name(), Self::from_query(binding));
                let scope = ScalarAnalysisContext::nested(&recursive_with, &from, outer);
                let recursive = SelectStatement::from_ast_in(set.right(), context, Some(&scope))?;
                SelectStatement::recursive_union(context, entry.name(), &aliases, anchor, recursive)
            },
        )?;
        query.rename_columns(&aliases);
        Ok(Self::from_query(query))
    }
}

#[cfg(test)]
mod test_recursive_ctes {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        sem::{CteStatement, PgBuiltInType},
        test_helpers::query::{output_type, resolve},
    };

    #[test]
    fn retains_analyzed_arms_and_declaration_order() {
        let (query, context) = resolve(
            "WITH RECURSIVE a AS (SELECT n FROM b),              b(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM b WHERE n<2) SELECT n FROM a",
            Dialect::postgres(),
            true,
        )
        .unwrap();
        assert!(
            context.reported_problems().is_empty(),
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(output_type(&query, "n"), Some(PgBuiltInType::int4()));
        let [first, second] = query.query_clauses().unwrap().with() else {
            panic!("two CTEs");
        };
        assert_eq!(first.name(), "a");
        let CteStatement::Select(recursive) = second.statement() else {
            panic!("retained recursive query");
        };
        assert!(recursive.query_clauses().unwrap().set_all());
    }

    #[test]
    fn recursive_and_forward_edges_keep_parameters_connected() {
        let (query, context) = resolve(
            "WITH RECURSIVE a AS (SELECT n FROM b),              b(n) AS (SELECT $1 UNION ALL SELECT n FROM b WHERE false) SELECT n FROM a",
            Dialect::postgres(),
            true,
        )
        .unwrap();
        assert!(
            context.reported_problems().is_empty(),
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(output_type(&query, "n"), Some(PgBuiltInType::text()));
        assert_eq!(
            context
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .resolved()
                .single_type(),
            Some(PgBuiltInType::text()),
        );
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn resolves_shared_sqlite_recursive_binding() {
        const SQL: &str = "WITH RECURSIVE a AS (SELECT n FROM B),              b(n) AS (SELECT 1 UNION ALL SELECT n FROM B WHERE false) SELECT n FROM a";
        let oracle = rusqlite::Connection::open_in_memory().unwrap();
        assert_eq!(
            oracle
                .query_row(SQL, [], |row| row.get::<_, i32>(0))
                .unwrap(),
            1
        );
        let (query, context) = resolve(SQL, Dialect::sqlite(), true).unwrap();
        assert!(
            context.reported_problems().is_empty(),
            "{:?}",
            context.reported_problems()
        );
        assert!(output_type(&query, "n").is_some());
    }
}
