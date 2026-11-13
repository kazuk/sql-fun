//! Applies project declarations to completed semantic references in one context.
use crate::sem::search_path::expand_relation_search_path;
use crate::{
    ast::Dialect,
    context::ParseContext,
    names::{ColumnName, FullName},
    sem::{
        AnalysisError, AnalysisProblem, ColumnCollection, SemAst, TypeReference,
        traversal::SemanticVisitor,
    },
};
use ::sql_fun_core::{
    DeprecatedLintItem,
    analysis_problem::{LintProblem, LintTarget},
};
use std::collections::HashMap;

/// Effective expectation and diagnostic occurrence for one configured target.
///
/// The index identifies the first declaration of a resolved target. Duplicate
/// declarations OR their expectation into that entry. Unresolved declarations
/// keep their own index and remain unobserved. No mutable period state is shared.
#[derive(Debug, Clone)]
pub struct DeprecatedObservation {
    index: usize,
    expect: bool,
    occurred: bool,
}
impl DeprecatedObservation {
    /// Index into the immutable deprecated declaration list.
    pub const fn index(&self) -> usize {
        self.index
    }
    /// Whether an expected deprecated diagnostic must occur during the period.
    pub const fn expected(&self) -> bool {
        self.expect
    }
    /// Whether the corresponding deprecated diagnostic was emitted.
    pub const fn occurred(&self) -> bool {
        self.occurred
    }
}

/// Observations from one explicit traversal, ready for period accumulation.
#[derive(Debug, Clone, Default)]
pub struct DeprecatedObservations(Vec<DeprecatedObservation>);
impl DeprecatedObservations {
    /// Effective configured expectations, including inapplicable declarations.
    pub fn entries(&self) -> &[DeprecatedObservation] {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Target {
    kind: String,
    name: FullName,
    column: Option<String>,
    label: Option<String>,
}
impl Target {
    fn new(kind: &str, name: &FullName, column: Option<&str>, label: Option<&str>) -> Self {
        Self {
            kind: kind.to_owned(),
            name: name.clone(),
            column: column.map(str::to_owned),
            label: label.map(str::to_owned),
        }
    }
    fn in_context(mut self, context: &impl ParseContext) -> Self {
        if context.dialect() == Dialect::sqlite() {
            self.name = self.name.sqlite_identity();
            self.column = self.column.map(|column| column.to_ascii_lowercase());
        }
        self
    }
    fn components(&self) -> Vec<String> {
        let mut result = Vec::new();
        if let Some(schema) = self.name.schema() {
            result.push(schema.as_str().to_owned());
        }
        result.push(self.name.local_name().as_str().to_owned());
        result
    }
    fn warning(&self) -> LintProblem {
        LintProblem::deprecated(&LintTarget::new(
            &self.kind,
            &self.components(),
            self.column.as_deref(),
            self.label.as_deref(),
        ))
    }
    fn resolve(item: &DeprecatedLintItem, context: &impl ParseContext) -> Result<Self, String> {
        let requested = match item.name() {
            [local] => FullName::from_local_name(local),
            [schema, local] => FullName::with_schema(schema, local),
            _ => {
                return Err(
                    "target requires one name component or schema and name components".to_owned(),
                );
            }
        };
        if item.column().is_some() && item.kind() != "table" && item.kind() != "view" {
            return Err("column is only applicable to table or view targets".to_owned());
        }
        if item.label().is_some() && item.kind() != "enum_label" {
            return Err("label is only applicable to enum_label targets".to_owned());
        }
        let name = match item.kind() {
            "table" | "view" => Self::resolve_relation(item, &requested, context)?,
            "function" => {
                context
                    .get_function_by_name(&requested)
                    .ok_or("configured function cannot be resolved")?;
                context.resolve_function_name_with_search_path(&requested)
            }
            "type" | "domain" | "enum_label" => Self::resolve_type(item, &requested, context)?,
            kind => return Err(format!("unknown deprecated target kind {kind:?}")),
        };
        Ok(Self::new(item.kind(), &name, item.column(), item.label()).in_context(context))
    }
    fn resolve_relation(
        item: &DeprecatedLintItem,
        requested: &FullName,
        context: &impl ParseContext,
    ) -> Result<FullName, String> {
        let name = if context.dialect() == Dialect::sqlite() {
            expand_relation_search_path(context, requested, |name| {
                context.sqlite_relation_identity(name)
            })
        } else {
            context.resolve_relation_name(requested)
        }
        .ok_or("configured relation cannot be resolved")?;
        if item.kind() == "table" {
            let table = context
                .get_table_impl(&name)
                .ok_or("configured target is not a table")?;
            if let Some(column) = item.column()
                && table
                    .find_column(
                        &ColumnName::from(column),
                        context.dialect() == Dialect::sqlite(),
                    )
                    .is_none()
            {
                return Err("configured table column cannot be resolved".to_owned());
            }
        } else {
            let view = context
                .get_view_impl(&name)
                .ok_or("configured target is not a view")?;
            if let Some(column) = item.column() {
                let columns = ColumnCollection::new(
                    view.result_columns()
                        .column_definitions()
                        .map_err(|error| error.to_string())?,
                );
                if columns
                    .find_column(
                        &ColumnName::from(column),
                        context.dialect() == Dialect::sqlite(),
                    )
                    .is_none()
                {
                    return Err("configured view column cannot be resolved".to_owned());
                }
            }
        }
        Ok(name)
    }
    fn resolve_type(
        item: &DeprecatedLintItem,
        requested: &FullName,
        context: &impl ParseContext,
    ) -> Result<FullName, String> {
        let name = context
            .resolve_type_name(requested)
            .ok_or("configured type cannot be resolved")?;
        let definition = context
            .get_type_impl(&name)
            .ok_or("configured type cannot be resolved")?;
        if item.kind() == "domain" && !definition.is_domain() {
            return Err("configured target is not a domain".to_owned());
        }
        if item.kind() == "type" && definition.is_domain() {
            return Err("configured target is a domain, not a type target".to_owned());
        }
        if item.kind() == "enum_label" {
            let label = item.label().ok_or("enum_label target requires a label")?;
            if !definition.has_enum_label(label) {
                return Err("configured enum label cannot be resolved".to_owned());
            }
        }
        Ok(name)
    }
}

struct DeprecatedHandler<'a, C: ParseContext> {
    context: &'a mut C,
    targets: HashMap<Target, usize>,
    observations: DeprecatedObservations,
}
impl<'a, C: ParseContext> DeprecatedHandler<'a, C> {
    fn new(context: &'a mut C) -> Result<Self, AnalysisError> {
        let configuration = context.lint_configuration().clone();
        let mut handler = Self {
            context,
            targets: HashMap::new(),
            observations: DeprecatedObservations::default(),
        };
        for (index, item) in configuration.deprecated().iter().enumerate() {
            let expect = item.expect() == Some(true);
            let diagnostic_target =
                LintTarget::new(item.kind(), item.name(), item.column(), item.label());
            match Target::resolve(item, handler.context) {
                Ok(target) => {
                    if let Some(position) = handler.targets.get(&target) {
                        handler.observations.0[*position].expect |= expect;
                        handler.report(LintProblem::duplicate(&diagnostic_target))?;
                    } else {
                        handler.targets.insert(target, handler.observations.0.len());
                        handler.observations.0.push(DeprecatedObservation {
                            index,
                            expect,
                            occurred: false,
                        });
                    }
                }
                Err(reason) => {
                    handler.report(LintProblem::configuration(&diagnostic_target, &reason))?;
                    handler.observations.0.push(DeprecatedObservation {
                        index,
                        expect,
                        occurred: false,
                    });
                }
            }
        }
        Ok(handler)
    }
    fn report(&mut self, problem: LintProblem) -> Result<(), AnalysisError> {
        self.context.report_problem(AnalysisProblem::lint(&problem))
    }
    fn reference(&mut self, target: Target) -> Result<(), AnalysisError> {
        let target = target.in_context(self.context);
        if let Some(position) = self.targets.get(&target).copied() {
            self.report(target.warning())?;
            self.observations.0[position].occurred = true;
        }
        Ok(())
    }
}
impl<C: ParseContext> SemanticVisitor for DeprecatedHandler<'_, C> {
    fn schema_relation_identity(&mut self, name: &FullName) -> Result<(), AnalysisError> {
        self.reference(Target::new("table", name, None, None))?;
        self.reference(Target::new("view", name, None, None))
    }

    fn schema_relation(&mut self, name: &FullName, view: bool) -> Result<(), AnalysisError> {
        self.reference(Target::new(
            if view { "view" } else { "table" },
            name,
            None,
            None,
        ))
    }
    fn schema_column(
        &mut self,
        name: &FullName,
        view: bool,
        column: &ColumnName,
    ) -> Result<(), AnalysisError> {
        self.reference(Target::new(
            if view { "view" } else { "table" },
            name,
            Some(column.as_str()),
            None,
        ))
    }
    fn schema_function(&mut self, name: &FullName) -> Result<(), AnalysisError> {
        self.reference(Target::new("function", name, None, None))
    }
    fn schema_type(&mut self, reference: &TypeReference) -> Result<(), AnalysisError> {
        let Some(name) = reference.full_name() else {
            return Ok(());
        };
        let kind = if self
            .context
            .get_type_impl(name)
            .is_some_and(|definition| definition.is_domain())
        {
            "domain"
        } else {
            "type"
        };
        self.reference(Target::new(kind, name, None, None))
    }
}
#[cfg(test)]
mod test_deprecated_handler {
    use crate::test_helpers::deprecated_lint;

    #[rstest::rstest]
    #[case::alias(
        "SELECT x.legacy FROM public.items AS x",
        "table",
        "items",
        Some("legacy")
    )]
    #[case::table_column_alias("SELECT b FROM items AS x(a,b)", "table", "items", Some("legacy"))]
    #[case::view_column_alias(
        "SELECT b FROM old_view AS x(a,b)",
        "view",
        "old_view",
        Some("legacy")
    )]
    #[case::create_view(
        "CREATE VIEW fresh AS SELECT legacy FROM items",
        "table",
        "items",
        Some("legacy")
    )]
    #[case::create_table_as(
        "CREATE TABLE fresh AS SELECT legacy FROM items",
        "table",
        "items",
        Some("legacy")
    )]
    #[case::declaration_type("CREATE TABLE fresh (x mood)", "type", "mood", None)]
    #[case::index_expression(
        "CREATE INDEX fresh ON items (legacy)",
        "table",
        "items",
        Some("legacy")
    )]
    #[case::alter_table(
        "ALTER TABLE items ALTER COLUMN legacy SET DEFAULT 'x'",
        "table",
        "items",
        None
    )]
    #[case::whole_table("SELECT 1 FROM items", "table", "items", None)]
    #[case::wildcard("SELECT * FROM items", "table", "items", Some("legacy"))]
    #[case::nested("SELECT (SELECT legacy FROM items)", "table", "items", Some("legacy"))]
    #[case::cte(
        "WITH x AS (SELECT legacy FROM items) SELECT * FROM x",
        "table",
        "items",
        Some("legacy")
    )]
    #[case::view_column("SELECT v.legacy FROM old_view v", "view", "old_view", Some("legacy"))]
    #[case::view("SELECT 1 FROM old_view", "view", "old_view", None)]
    #[case::function_int("SELECT legacy(1)", "function", "legacy", None)]
    #[case::function_text("SELECT legacy('x'::text)", "function", "legacy", None)]
    #[case::type_target("SELECT 'happy'::mood", "type", "mood", None)]
    #[case::domain("SELECT 1::legacy_domain", "domain", "legacy_domain", None)]
    #[case::insert(
        "INSERT INTO items (legacy) VALUES ('x')",
        "table",
        "items",
        Some("legacy")
    )]
    #[case::update("UPDATE items SET legacy = 'x'", "table", "items", Some("legacy"))]
    #[case::delete("DELETE FROM items RETURNING legacy", "table", "items", Some("legacy"))]
    fn observes_resolved_references(
        #[case] sql: &str,
        #[case] kind: &str,
        #[case] name: &str,
        #[case] column: Option<&str>,
    ) -> testresult::TestResult {
        let config = deprecated_lint::target(kind, name, column);
        let mut pair = deprecated_lint::analyze(sql, &config)?;
        assert!(
            !pair
                .context()
                .problems()
                .iter()
                .any(|problem| problem.code() == "deprecated")
        );
        let observations = pair.lint_deprecated()?;
        assert!(observations.entries()[0].expected());
        assert!(
            observations.entries()[0].occurred(),
            "{:?}",
            pair.context().problems()
        );
        assert!(
            pair.context()
                .problems()
                .iter()
                .any(|problem| problem.code() == "deprecated")
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::column_not_whole("SELECT id FROM items", "table", "items", Some("legacy"))]
    #[case::view_not_definition("SELECT legacy FROM old_view", "table", "items", None)]
    #[case::table_not_view("SELECT legacy FROM items", "view", "old_view", None)]
    #[case::unrelated_type("SELECT 1", "type", "mood", None)]
    fn unrelated_references_do_not_satisfy_expectations(
        #[case] sql: &str,
        #[case] kind: &str,
        #[case] name: &str,
        #[case] column: Option<&str>,
    ) -> testresult::TestResult {
        let config = deprecated_lint::target(kind, name, column);
        let mut pair = deprecated_lint::analyze(sql, &config)?;
        let observations = pair.lint_deprecated()?;
        assert!(observations.entries()[0].expected());
        assert!(!observations.entries()[0].occurred());
        assert!(
            !pair
                .context()
                .problems()
                .iter()
                .any(|problem| problem.code() == "deprecated")
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::true_false("true", "false", true)]
    #[case::false_true("false", "true", true)]
    #[case::true_omitted("true", "", true)]
    #[case::omitted_true("", "true", true)]
    #[case::false_omitted("false", "", false)]
    #[case::omitted_false("", "false", false)]
    fn duplicate_targets_warn_and_preserve_strongest_expectation(
        #[case] first: &str,
        #[case] second: &str,
        #[case] expected: bool,
    ) -> testresult::TestResult {
        let config = deprecated_lint::duplicate_targets(first, second);
        let mut pair = deprecated_lint::analyze("SELECT 1", &config)?;
        let observations = pair.lint_deprecated()?;
        assert_eq!(observations.entries().len(), 1);
        assert_eq!(observations.entries()[0].expected(), expected);
        assert!(!observations.entries()[0].occurred());
        let warnings: Vec<_> = pair
            .context()
            .problems()
            .iter()
            .filter(|problem| problem.code() == "duplicate_lint_target")
            .collect();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].severity(),
            ::sql_fun_core::analysis_problem::DiagnosticSeverity::Warning
        );
        Ok(())
    }

    #[test]
    fn enum_type_reference_does_not_substitute_for_label_reference() -> testresult::TestResult {
        let mut pair = deprecated_lint::analyze(
            "SELECT NULL::mood",
            r#"[[lint.deprecated]]
kind = "enum_label"
name = ["mood"]
label = "sad"
expect = true
"#,
        )?;
        let observations = pair.lint_deprecated()?;
        assert!(observations.entries()[0].expected());
        assert!(!observations.entries()[0].occurred());
        assert!(pair.context().problems().is_empty());
        Ok(())
    }

    #[rstest::rstest]
    #[case::table_is_view("table", "old_view", "")]
    #[case::domain_is_type("domain", "mood", "")]
    #[case::type_is_domain("type", "legacy_domain", "")]
    #[case::required_label("enum_label", "mood", "")]
    #[case::missing_column("table", "items", "column = \"missing\"")]
    #[case::missing_label("enum_label", "mood", "label = \"missing\"")]
    #[case::wrong_field("function", "legacy", "column = \"legacy\"")]
    #[case::unknown_kind("future_kind", "items", "")]
    #[case::missing_object("table", "missing", "")]
    fn invalid_entries_do_not_broaden_targets_or_disable_other_rules(
        #[case] kind: &str,
        #[case] name: &str,
        #[case] extra: &str,
    ) -> testresult::TestResult {
        let config = format!(
            r#"[[lint.deprecated]]
kind = {kind:?}
name = [{name:?}]
{extra}
expect = true
[[lint.deprecated]]
kind = "table"
name = ["items"]
column = "legacy"
"#
        );
        let mut pair = deprecated_lint::analyze("SELECT legacy FROM items", &config)?;
        let observations = pair.lint_deprecated()?;
        assert!(observations.entries()[0].expected());
        assert!(!observations.entries()[0].occurred());
        assert!(observations.entries()[1].occurred());
        let warning = pair
            .context()
            .problems()
            .iter()
            .find(|problem| problem.code() == "lint_configuration")
            .unwrap();
        assert_eq!(
            warning.severity(),
            ::sql_fun_core::analysis_problem::DiagnosticSeverity::Warning
        );
        assert!(warning.statement_relative_span().is_none());
        Ok(())
    }
}

impl SemAst {
    /// Explicitly apply project deprecated rules using this statement's analysis context.
    ///
    /// The caller determines project applicability. Ordinary semantic analysis
    /// never invokes this operation. Resolved targets are local to this call.
    /// # Errors
    /// Returns an error when traversal or diagnostic delivery fails.
    pub(crate) fn lint_deprecated(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<DeprecatedObservations, AnalysisError> {
        let mut handler = DeprecatedHandler::new(context)?;
        self.visit_references(&mut handler)?;
        Ok(handler.observations)
    }
}
