use crate::{
    ast::{Expr, TryFromWithTokens},
    context::ParseContext,
    names::{AliasName, ColumnName as NamesColumnName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, DataSource, FromClause, FuncCallExpr, Nullable,
        QueryName, ScalarAnalysisContext, SemScalarExpr, TableFunctionProblem,
        WildcardRowsFunction, WildcardRowsSource, WithClause,
        data_source::{
            ColumnCandidates, ProjectionColumn, TableFunctionOutput,
            function_columns::OutputColumnNames, projection::ProjectionColumns,
        },
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
    syn::{FuncCall, Node, ScanToken},
};

use ::sql_fun_core::IVec;

/// row functions
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DataSourceRowFunctions {
    alias: AliasSpec,
    functions: IVec<DataSourceRowsFromFunc>,
    ordinality: bool,
    outer_definition: TableFunctionOutput,
}

impl DataSourceRowFunctions {
    pub(super) fn projection_shape_is_resolved(&self) -> bool {
        self.functions
            .iter()
            .all(DataSourceRowsFromFunc::projection_shape_is_resolved)
    }
    pub(super) fn projection_is_table_dependent(&self, context: &impl ParseContext) -> bool {
        self.functions
            .iter()
            .any(|function| function.projection_is_table_dependent(context))
    }

    pub(super) fn matches_alias(&self, name: &AliasName) -> bool {
        self.alias.name() == name
    }

    pub(super) fn resolve_alias<'a>(
        &self,
        data_source: &'a DataSource,
        name: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        if self.alias.name() == name {
            Some((data_source, false))
        } else {
            None
        }
    }

    pub(super) fn new(functions: &IVec<DataSourceRowsFromFunc>, alias: &AliasSpec) -> Self {
        Self {
            functions: functions.clone(),
            ordinality: false,
            outer_definition: TableFunctionOutput::default(),
            alias: alias.clone(),
        }
    }

    /// Calls supplied to ROWS FROM, in written order.
    pub const fn functions(&self) -> &IVec<DataSourceRowsFromFunc> {
        &self.functions
    }

    /// Names retained on resolved invocations, in call order.
    /// Deferred or unresolved calls without a resolved invocation are omitted.
    pub fn resolved_function_names(&self) -> Vec<QueryName> {
        self.functions
            .iter()
            .filter_map(|function| function.source()?.resolved_function_name().cloned())
            .collect()
    }

    /// get alias name
    #[must_use]
    pub fn get_aliases(&self) -> Vec<AliasSpec> {
        vec![self.alias.clone()]
    }
}

/// One ROWS FROM invocation, retaining its signature and output shape.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DataSourceRowsFromFunc {
    source: RowsFromSource,
}
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum RowsFromSource {
    Invocation(Box<DataSource>),
    Expression(Box<RowsFromExpression>),
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
struct RowsFromExpression {
    value: SemScalarExpr,
    name: String,
    invalid_expression: bool,
    #[serde(default)]
    placement_pending: bool,
    columns: Vec<NamesColumnName>,
    output: TableFunctionOutput,
}

impl DataSourceRowsFromFunc {
    fn projection_shape_is_resolved(&self) -> bool {
        self.source()
            .is_none_or(DataSource::projection_shape_is_resolved)
    }
    fn projection_is_table_dependent(&self, context: &impl ParseContext) -> bool {
        self.source()
            .is_some_and(|source| source.projection_is_table_dependent(context))
    }
    fn column_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<NamesColumnName>, AnalysisError> {
        match &self.source {
            RowsFromSource::Invocation(source) => source.column_names(context, with),
            RowsFromSource::Expression(source) => Ok(source.columns.clone()),
        }
    }
    fn wildcard_layout(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<WildcardRowsFunction, AnalysisError> {
        Ok(WildcardRowsFunction::from_columns(
            &self.column_names(context, with)?,
        ))
    }
    pub(crate) fn invocation(source: &DataSource) -> Self {
        Self {
            source: RowsFromSource::Invocation(Box::new(source.clone())),
        }
    }
    pub(crate) fn expression(
        value: &SemScalarExpr,
        name: &str,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut value = value.clone();
        let invalid_expression = Self::validate_expression(context, &mut value)?;
        let placement_pending = value.table_root_depth().1;
        let columns = value
            .composite_columns(context)
            .map(|columns| columns.names())
            .unwrap_or_else(|| vec![crate::names::ColumnName::from(name)]);
        Ok(Self {
            source: RowsFromSource::Expression(Box::new(RowsFromExpression {
                value,
                name: name.to_owned(),
                invalid_expression,
                placement_pending,
                columns,
                output: TableFunctionOutput::default(),
            })),
        })
    }
    pub(crate) fn apply_output(
        &mut self,
        context: &mut impl ParseContext,
        output: &TableFunctionOutput,
    ) -> Result<(), AnalysisError> {
        match &mut self.source {
            RowsFromSource::Invocation(source) => source.apply_function_output(context, output),
            RowsFromSource::Expression(source) => source.apply_output(context, output),
        }
    }
    pub(crate) fn default_alias(&self) -> String {
        match &self.source {
            RowsFromSource::Invocation(source) => source.default_function_alias(),
            RowsFromSource::Expression(source) => source.name.clone(),
        }
    }
    /// Preserved invocation when its signature is deferred.
    pub fn deferred_function(&self) -> Option<&FuncCallExpr> {
        self.source()?
            .deferred_function()
            .map(|source| source.call())
    }
    /// Function source, absent for SQL-value and special-expression elements.
    pub fn source(&self) -> Option<&DataSource> {
        match &self.source {
            RowsFromSource::Invocation(source) => Some(source),
            _ => None,
        }
    }
    /// Whether a cast's FROM placement still depends on an unresolved call result.
    pub fn has_pending_placement(&self) -> bool {
        if let RowsFromSource::Expression(source) = &self.source {
            source.placement_pending
        } else {
            false
        }
    }
    /// Whether expression placement validation rejected this special element.
    /// Name/type lookup outcomes remain retained in the expression independently.
    pub fn has_invalid_expression(&self) -> bool {
        match &self.source {
            RowsFromSource::Expression(source) => source.invalid_expression,
            _ => false,
        }
    }
    /// Analyzed SQL-value or special expression supplied as a FROM element.
    pub fn value(&self) -> Option<&SemScalarExpr> {
        match &self.source {
            RowsFromSource::Expression(source) => Some(&source.value),
            _ => None,
        }
    }
    /// analyze `FuncCall` node in RangeFunction.functions
    pub fn analyze_func_call<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        func_call: FuncCall,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = Expr::try_from_with_tokens(&func_call, tokens)?;
        let result = syntax.resolve_rows(
            &mut context,
            &ScalarAnalysisContext::new(with_clause, from_clause),
        );
        Ok((result?, context))
    }

    /// analyze Node in RangeFunction.functions
    pub fn analyze_node<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        function: Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = Expr::try_from_with_tokens(&function, tokens)?;
        let result = syntax.resolve_rows(
            &mut context,
            &ScalarAnalysisContext::new(with_clause, from_clause),
        );
        Ok((result?, context))
    }
}

#[cfg(test)]
mod test_expression_sources {
    use crate::sem::DataSourceRowsFromFunc;

    use crate::{
        context::ParseContext,
        names::AliasName,
        sem::{DataSourceRowFunctions, PgBuiltInType},
        test_helpers::{rows_from, sampling},
    };

    #[test]
    fn retains_expression_binding_and_serialization() {
        let mut context = sampling::context();
        let query = sampling::query(
            "SELECT r.v FROM ROWS FROM(CAST(1 AS int)) r(v)",
            &mut context,
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let source = rows_from::source(&query, "r");
        assert!(source.functions()[0].source().is_none());
        assert!(source.functions()[0].value().is_some());
        let restored: DataSourceRowFunctions =
            ::serde_json::from_str(&::serde_json::to_string(source).unwrap()).unwrap();
        assert_eq!(&restored, source);
        let column = query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_source_column()
            .unwrap();
        assert_eq!(column.alias(), &AliasName::from("r"));
        assert_eq!(column.name().as_str(), "v");
        assert_eq!(column.value().get_type(), Some(PgBuiltInType::int4()));
    }

    #[rstest::rstest]
    #[case::sql_value("CURRENT_DATE")]
    #[case::coalesce("COALESCE(NULL::int,1)")]
    #[case::cast("CAST(1 AS int)")]
    fn legacy_entry_retains_special_expression(#[case] expression: &str) {
        let source = rows_from::legacy_element(expression, sampling::context());
        assert!(source.source().is_none());
        assert!(source.value().is_some());
        assert!(!source.has_invalid_expression());
    }

    #[test]
    fn preserves_existing_invocation_wire_shape() {
        let mut context = sampling::context();
        let query =
            sampling::query("SELECT r.v FROM ROWS FROM(scalar507(1)) r(v)", &mut context).unwrap();
        let source = rows_from::source(&query, "r");
        let value = ::serde_json::to_value(&source.functions()[0]).unwrap();
        assert!(value["source"].get("CallTableFunc").is_some());
        let restored: DataSourceRowsFromFunc = ::serde_json::from_value(value).unwrap();
        assert_eq!(&restored, &source.functions()[0]);
    }

    #[test]
    fn retains_rejected_expression_and_its_operands() {
        let mut context = sampling::context();
        let query = sampling::query(
            "SELECT 1 FROM ROWS FROM(COALESCE(set507(1),0)) r",
            &mut context,
        )
        .unwrap();
        let source = rows_from::source(&query, "r");
        assert!(source.functions()[0].has_invalid_expression());
        assert!(matches!(
            source.functions()[0].value().unwrap(),
            crate::sem::SemScalarExpr::Coalesce(_)
        ));
        let restored: DataSourceRowFunctions =
            ::serde_json::from_str(&::serde_json::to_string(source).unwrap()).unwrap();
        assert_eq!(&restored, source);
    }

    #[rstest::rstest]
    #[case::plain("1", false)]
    #[case::nested_set("set507(1)", true)]
    fn named_placement_resolves_signature_and_retains_known_violations(
        #[case] argument: &str,
        #[case] invalid: bool,
    ) {
        let mut context = rows_from::deferred_returns_context();
        let query = sampling::query(
            &format!("SELECT 1 FROM ROWS FROM(CAST(pending508(n => {argument}) AS int)) r"),
            &mut context,
        )
        .unwrap();
        let source = rows_from::source(&query, "r");
        let element = &source.functions()[0];
        assert!(!element.has_pending_placement());
        assert_eq!(element.has_invalid_expression(), invalid);
        assert_eq!(context.reported_problem_count(), usize::from(invalid));
        let restored: DataSourceRowFunctions =
            ::serde_json::from_str(&::serde_json::to_string(source).unwrap()).unwrap();
        assert_eq!(&restored, source);
    }

    #[test]
    fn visits_unselected_operands_and_preserves_parameter_domains() {
        let mut context = sampling::context();
        let query = sampling::query(
            "SELECT r.v FROM ROWS FROM(COALESCE($1::int,1),CAST($2 AS int)) r(v,unused)",
            &mut context,
        )
        .unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let constraints = context.parameter_constraints();
        assert!(constraints.allows(&std::collections::BTreeMap::from([
            (1, PgBuiltInType::int4()),
            (2, PgBuiltInType::int4())
        ])));
        assert!(!constraints.allows(&std::collections::BTreeMap::from([
            (1, PgBuiltInType::bool()),
            (2, PgBuiltInType::int4())
        ])));
        let parameters: std::collections::BTreeMap<_, _> =
            rows_from::parameter_bindings(&query).into_iter().collect();
        assert_eq!(
            parameters,
            std::collections::BTreeMap::from([
                (1, Some(PgBuiltInType::int4())),
                (2, Some(PgBuiltInType::int4())),
            ])
        );
        assert_eq!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .is_not_null(),
            &Some(false)
        );
    }
}

impl DataSourceRowFunctions {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        let mut functions = self.functions.to_vec();
        for function in &mut functions {
            function.walk(visitor)?;
        }
        self.functions = functions.into();
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        visitor.enter_rows_functions();
        for function in self.functions.iter() {
            function.visit(visitor)?;
        }
        visitor.leave_rows_functions(&self.alias, self.ordinality);
        Ok(())
    }
}
impl DataSourceRowsFromFunc {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match &mut self.source {
            RowsFromSource::Invocation(source) => source.walk(visitor)?,
            RowsFromSource::Expression(source) => source.value.walk(visitor)?,
        }
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        match &self.source {
            RowsFromSource::Invocation(source) => source.visit(visitor)?,
            RowsFromSource::Expression(source) => source.value.visit(visitor)?,
        }
        Ok(())
    }
}

impl DataSourceRowFunctions {
    pub(super) fn project<'a>(
        &'a self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<ProjectionColumns<'a>, AnalysisError> {
        let mut columns = ProjectionColumns::default();
        for function in &self.functions {
            function.append_projection(context, with, &mut columns)?;
        }
        // Independent set lengths can null-extend every constituent output.
        let mut columns = columns.with_occurrence(None, self.functions.len() > 1);
        if self.ordinality {
            columns.append_ordinality(&self.alias);
        }
        let columns = columns.with_aliases(&self.alias);
        if self.projection_is_table_dependent(context) {
            let functions = self
                .functions
                .iter()
                .map(|function| function.wildcard_layout(context, with))
                .collect::<Result<Vec<_>, _>>()?;
            let source = WildcardRowsSource::new(
                self.alias.name(),
                &functions,
                &columns.names(),
                self.ordinality,
            );
            Ok(columns.with_rows_source(&source))
        } else {
            Ok(columns)
        }
    }
    pub(super) fn column_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<NamesColumnName>, AnalysisError> {
        Ok(self.project(context, with)?.names())
    }
    fn known_column_names(&self) -> Option<OutputColumnNames> {
        let mut names = OutputColumnNames::default();
        for function in &self.functions {
            function.append_known_column_names(&mut names)?;
        }
        if self.ordinality {
            names.append_ordinality();
        }
        Some(names)
    }
    pub(super) fn candidates(&self, name: &NamesColumnName) -> ColumnCandidates {
        let Some(names) = self.known_column_names() else {
            return ColumnCandidates::unknown();
        };
        ColumnCandidates::from(names.candidates(name, &self.alias))
    }
    pub(super) fn bind_column<'a, C: ParseContext>(
        &self,
        context: &'a mut C,
        with: &WithClause,
        alias: &AliasName,
        name: &NamesColumnName,
        nullable: bool,
    ) -> Result<(SemScalarExpr, &'a mut C), AnalysisError> {
        let expression = self
            .project(context, with)?
            .bind_column(context, with, alias, name, nullable)?;
        Ok((expression, context))
    }
    pub(super) fn validate_aliases(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        let Some(names) = self.known_column_names() else {
            return Ok(());
        };
        self.outer_definition.validate_alias_count(
            context,
            self.alias.len_derived_column(),
            names.len(),
        )
    }
}
impl DataSourceRowsFromFunc {
    fn append_projection<'a>(
        &'a self,
        context: &impl ParseContext,
        with: &WithClause,
        columns: &mut ProjectionColumns<'a>,
    ) -> Result<(), AnalysisError> {
        match &self.source {
            RowsFromSource::Invocation(source) => columns.append_source(source, context, with),
            RowsFromSource::Expression(source) => source.append_projection(context, columns),
        }
    }
}

impl DataSourceRowFunctions {
    pub(super) const fn append_ordinality(&mut self) {
        self.ordinality = true;
    }
}

impl DataSourceRowFunctions {
    pub(super) fn retain_outer_definition(&mut self, definition: &TableFunctionOutput) {
        self.outer_definition = definition.clone();
    }
}

impl DataSourceRowsFromFunc {
    fn append_known_column_names(&self, names: &mut OutputColumnNames) -> Option<()> {
        match &self.source {
            RowsFromSource::Invocation(source) => names.append_function_columns(source),
            RowsFromSource::Expression(source) => source.append_known_column_names(names),
        }
    }
}

impl DataSourceRowFunctions {
    pub(super) fn projection_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        qualifier: Option<&AliasName>,
    ) -> Result<Vec<ProjectionColumn<'_>>, AnalysisError> {
        if qualifier.is_some_and(|name| !self.matches_alias(name)) {
            return Ok(Vec::new());
        }
        Ok(self.project(context, with)?.into_columns())
    }
}

impl DataSourceRowFunctions {
    pub(super) fn apply_output(
        &mut self,
        context: &mut impl ParseContext,
        output: &TableFunctionOutput,
    ) -> Result<(), AnalysisError> {
        let mut functions = self.functions.to_vec();
        for function in &mut functions {
            function.apply_output(context, output)?;
        }
        self.functions = functions.into();
        self.outer_definition = output.clone();
        Ok(())
    }
}

struct FromExpressionValidator<'a, C> {
    context: &'a C,
    query_depth: usize,
    expression_depth: usize,
    root_depth: usize,
    invalid: bool,
}
impl<C: ParseContext> crate::sem::traversal::SemanticVisitorMut for FromExpressionValidator<'_, C> {
    fn walk_source_column_values(&self) -> bool {
        false
    }
    fn enter_expression(&mut self, _: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        self.expression_depth += 1;
        Ok(())
    }
    fn enter_query(&mut self) {
        self.query_depth += 1;
    }
    fn leave_query(&mut self) {
        self.query_depth -= 1;
    }
    fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if self.query_depth == 0
            && value.invalid_from_scalar(self.context, self.expression_depth <= self.root_depth)
        {
            self.invalid = true;
        }
        self.expression_depth -= 1;
        Ok(())
    }
}
impl DataSourceRowsFromFunc {
    pub(crate) fn validate_expression(
        context: &mut impl ParseContext,
        value: &mut SemScalarExpr,
    ) -> Result<bool, AnalysisError> {
        Self::validate_value(context, value, value.table_root_depth().0)
    }
    pub(crate) fn validate_argument(
        context: &mut impl ParseContext,
        value: &mut SemScalarExpr,
    ) -> Result<bool, AnalysisError> {
        Self::validate_value(context, value, 0)
    }
    fn validate_value(
        context: &mut impl ParseContext,
        value: &mut SemScalarExpr,
        root_depth: usize,
    ) -> Result<bool, AnalysisError> {
        let mut visitor = FromExpressionValidator {
            context: &*context,
            query_depth: 0,
            expression_depth: 0,
            root_depth,
            invalid: false,
        };
        value.walk(&mut visitor)?;
        let invalid = visitor.invalid;
        if invalid {
            context.report_analysis_problem(AnalysisProblem::table_function(
                TableFunctionProblem::invalid_expression(),
            ))?;
        }
        Ok(invalid)
    }
}

impl DataSource {
    fn default_function_alias(&self) -> String {
        self.get_aliases()
            .first()
            .map(|alias| alias.name().as_str().to_owned())
            .unwrap_or_default()
    }
}

impl RowsFromExpression {
    fn apply_output(
        &mut self,
        context: &mut impl ParseContext,
        output: &TableFunctionOutput,
    ) -> Result<(), AnalysisError> {
        self.output = output.clone();
        self.output.validate_requirement(context, false)
    }
    fn append_projection(
        &self,
        context: &impl ParseContext,
        columns: &mut ProjectionColumns<'_>,
    ) -> Result<(), AnalysisError> {
        columns.append_expression(&self.value, &self.name, context);
        Ok(())
    }
    fn append_known_column_names(&self, names: &mut OutputColumnNames) -> Option<()> {
        names.append_names(&self.columns);
        Some(())
    }
}
