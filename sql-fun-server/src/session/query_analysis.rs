//! Query-local analysis over a retained schema; no catalog discovery or inference solver.
use super::facade::Session;
use crate::InitializedSchema;
use ::sql_fun_core::{
    DiagnosticPresentation, PlaceholderedSql, analysis_problem::QueryAnalysisStage,
};
use ::sql_fun_server_api::{
    Collection, SqlFunServerApiError,
    payloads::{
        AnalysisProbrem, AnalyzeQueryArgs, AnalyzeQueryItem, AnalyzeQueryResponse,
        CollectionHandle, ColumnAccess, DiagnosticSeverity, Nullability, ObjectKind,
        ParameterDescribe, QueryKind, RelationName, ResultSetColumn, StatementByteRange, TextSpan,
        TypeReference,
    },
};
use ::sql_fun_sqlast::{
    CacheContext,
    ast::{Dialect, Statement},
    context::{BaseParseContext, ParameterTypeHintWrite, ParseContext, SchemaFileContext},
    parse_statements,
    sem::{AnalysisProblem, QueryMetadata, TypeReference as SqlType},
};
use std::path::{Path, PathBuf};

struct QuerySource {
    text: String,
    path: PathBuf,
}

impl QuerySource {
    fn from_request(args: &AnalyzeQueryArgs) -> Result<Self, SqlFunServerApiError> {
        let input = args.query();
        if let Some(text) = input.text() {
            Ok(Self {
                text: text.to_owned(),
                path: PathBuf::from("<query>"),
            })
        } else if let Some(path) = input.file() {
            let text = std::fs::read_to_string(path)
                .map_err(|error| SqlFunServerApiError::new_unexpected(&error.to_string()))?;
            Ok(Self {
                text,
                path: path.to_path_buf(),
            })
        } else {
            Err(SqlFunServerApiError::invalid_analysis_input(
                "URL query input is not supported",
            ))
        }
    }
}

/// Binding metadata shared across all statements in an explicitly supplied input.
struct Bindings {
    names: Vec<String>,
    hints: Vec<Option<String>>,
}

impl Bindings {
    fn type_hints(&self) -> Vec<Option<&str>> {
        self.hints.iter().map(Option::as_deref).collect()
    }

    fn from_query(query: &PlaceholderedSql) -> Result<Self, String> {
        let mut names = Vec::new();
        query.append_parameter_order(&mut names);
        let mut hints = Vec::new();
        for name in &names {
            let mut hint: Option<String> = None;
            for (_, occurrences) in query
                .parameters()
                .filter(|(parameter, _)| *parameter == name)
            {
                for occurrence in occurrences {
                    let next = occurrence.hint_value();
                    if hint.as_deref().is_some_and(|previous| previous != next) {
                        return Err(format!("conflicting SQL type hints for {name:?}"));
                    }
                    hint = Some(next.to_owned());
                }
            }
            hints.push(hint);
        }
        Ok(Self {
            names: names
                .into_iter()
                .map(|name| name.as_str().to_owned())
                .collect(),
            hints,
        })
    }
    fn from_realized(query: &crate::cte_realization::RealizedCteQuery) -> Self {
        Self {
            names: query
                .bindings()
                .iter()
                .map(|binding| binding.name().as_str().to_owned())
                .collect(),
            hints: query.bindings().type_hints(),
        }
    }
    fn render(&self, query: &PlaceholderedSql, dialect: Dialect) -> Result<String, String> {
        if !self.names.is_empty() && dialect == Dialect::postgres() {
            for token in query
                .tokens()
                .filter(|token| token.parameter_name().is_none())
            {
                let text = PlaceholderedSql::from_tokens(vec![token.clone()]).encode();
                if let Ok(tokens) = ::sql_fun_sqlast::PostgresDialect::scan(&text) {
                    for token in &tokens {
                        let value = &text[usize::try_from(token.get_start())
                            .expect("scanner start is nonnegative")
                            ..usize::try_from(token.get_end())
                                .expect("scanner end is nonnegative")];
                        if value
                            .strip_prefix('$')
                            .is_some_and(|number| number.parse::<u32>().is_ok())
                        {
                            return Err("named and native numbered parameters cannot share an input binding order".to_owned());
                        }
                    }
                }
            }
        }
        let names: Vec<_> = self
            .names
            .iter()
            .map(|name| ::sql_fun_core::ParameterName::from(name.as_str()))
            .collect();
        dialect
            .render_placeholdered_sql(query, &names.iter().collect::<Vec<_>>())
            .map_err(|error| error.to_string())
    }
    fn spans(&self, query: &PlaceholderedSql, dialect: Dialect) -> Result<SourceMap, String> {
        let mut map = SourceMap::default();
        for token in query.tokens() {
            let piece = PlaceholderedSql::from_tokens(vec![token.clone()]);
            let original = piece.encode();
            let rendered = self.render(&piece, dialect)?;
            let parameter = if let Some(name) = token.parameter_name() {
                self.names
                    .iter()
                    .position(|value| value == name.as_str())
                    .map(|slot| u32::try_from(slot + 1).expect("SQL parameter slot fits u32"))
            } else {
                None
            };
            map.push(&original, &rendered, parameter);
        }
        Ok(map)
    }
    fn name(&self, index: u32) -> String {
        // Invalid native $0 is diagnosed by SQLAST; it has no application binding.
        if index == 0 {
            return "$0".to_owned();
        }
        self.names
            .get(index as usize - 1)
            .cloned()
            .unwrap_or_else(|| format!("${index}"))
    }
}

#[derive(Default)]
struct SourceMap {
    original: String,
    rendered: String,
    segments: Vec<SourceSegment>,
    expanded: bool,
}
struct SourceSegment {
    original_start: usize,
    original_end: usize,
    rendered_start: usize,
    rendered_end: usize,
    parameter: Option<u32>,
}
impl SourceMap {
    fn push(&mut self, original: &str, rendered: &str, parameter: Option<u32>) {
        let segment = SourceSegment {
            original_start: self.original.len(),
            original_end: self.original.len() + original.len(),
            rendered_start: self.rendered.len(),
            rendered_end: self.rendered.len() + rendered.len(),
            parameter,
        };
        self.original.push_str(original);
        self.rendered.push_str(rendered);
        self.segments.push(segment);
    }
    fn original_offset(&self, offset: usize) -> usize {
        for segment in &self.segments {
            if offset >= segment.rendered_start && offset <= segment.rendered_end {
                if offset == segment.rendered_end {
                    return segment.original_end;
                }
                if segment.parameter.is_some() {
                    return segment.original_start;
                }
                return segment.original_start + offset - segment.rendered_start;
            }
        }
        self.original.len()
    }
    fn statement_span(&self, statement: &Statement) -> TextSpan {
        TextSpan::from_offsets(
            &self.original,
            self.original_offset(statement.span().offset()),
            self.original_offset(statement.span().offset() + statement.span().len()),
        )
    }
    fn parameter_spans(&self, statement: &Statement, index: u32) -> Vec<TextSpan> {
        let start = statement.span().offset();
        let end = start + statement.span().len();
        let mut spans = Vec::new();
        for segment in &self.segments {
            if segment.parameter == Some(index)
                && segment.rendered_start >= start
                && segment.rendered_end <= end
            {
                spans.push(TextSpan::from_offsets(
                    &self.original,
                    segment.original_start,
                    segment.original_end,
                ));
            }
        }
        // The dialect parser's scanner identifies native numbered parameters,
        // excluding literals/comments without a second SQL lexer.
        if let Ok(tokens) = ::sql_fun_sqlast::PostgresDialect::scan(&self.rendered[start..end]) {
            for token in &tokens {
                let from = start
                    + usize::try_from(token.get_start()).expect("scanner start is nonnegative");
                let to =
                    start + usize::try_from(token.get_end()).expect("scanner end is nonnegative");
                if self.rendered.get(from..to) == Some(format!("${index}").as_str())
                    && !self.segments.iter().any(|segment| {
                        segment.parameter.is_some()
                            && from >= segment.rendered_start
                            && to <= segment.rendered_end
                    })
                {
                    spans.push(TextSpan::from_offsets(
                        &self.original,
                        self.original_offset(from),
                        self.original_offset(to),
                    ));
                }
            }
        }
        spans
    }
}

#[derive(Default)]
struct QueryResult {
    span: Option<TextSpan>,
    kind: Option<QueryKind>,
    sql: Option<String>,
    complete: bool,
    problems: Vec<AnalysisProbrem>,
    parameters: Vec<ParameterDescribe>,
    columns: Vec<ResultSetColumn>,
    accesses: Vec<ColumnAccess>,
}

impl QueryResult {
    fn failure(
        stage: QueryAnalysisStage,
        message: &str,
        source: &QuerySource,
        dialect: ::sql_fun_core::SqlDialect,
    ) -> Self {
        Self {
            span: None,
            kind: None,
            sql: None,
            complete: false,
            problems: vec![diagnostic(
                &AnalysisProblem::query_analysis(stage, message),
                &source.path,
                Some(&source.text),
                dialect,
                0,
            )],
            parameters: Vec::new(),
            columns: Vec::new(),
            accesses: Vec::new(),
        }
    }
    fn parse_failure(
        error: &::sql_fun_sqlast::ParseStatementError,
        source: &QuerySource,
        schema: &InitializedSchema,
        mapping: &SourceMap,
    ) -> Self {
        let start = error.span().offset();
        let end = start + error.span().len();
        let stage = if error.is_syntax() {
            QueryAnalysisStage::parse()
        } else {
            QueryAnalysisStage::statement()
        };
        let dialect = schema.base_context().metadata().sql_dialect();
        let mut result = Self::failure(stage, &error.to_string(), source, dialect);
        let original_start = mapping.original_offset(start);
        let original_end = mapping.original_offset(end);
        result.span = Some(TextSpan::from_offsets(
            &mapping.original,
            original_start,
            original_end,
        ));
        if !error.is_syntax() {
            result.kind = Some(classify_kind(error.statement_kind()));
        }
        result.problems = vec![diagnostic(
            &AnalysisProblem::query_analysis(stage, &error.to_string()),
            &source.path,
            Some(&mapping.original[original_start..original_end]),
            dialect,
            original_start,
        )];
        result
    }

    fn report(&mut self, input: &StatementAnalysis<'_>, stage: QueryAnalysisStage, error: &str) {
        self.problems.push(input.diagnostic(stage, error));
    }

    fn clear_output(
        &mut self,
        input: &StatementAnalysis<'_>,
        stage: QueryAnalysisStage,
        error: &str,
    ) {
        self.sql = None;
        self.report(input, stage, error);
    }

    fn for_statement(statement: &Statement, mapping: &SourceMap) -> Self {
        Self {
            span: Some(mapping.statement_span(statement)),
            kind: Some(query_kind(statement)),
            ..Self::default()
        }
    }
}

/// Coordinates query-local output preparation without replacing the shared parse context.
struct StatementAnalysis<'a> {
    statement: &'a Statement,
    bindings: &'a Bindings,
    source: &'a QuerySource,
    schema: &'a InitializedSchema,
    mapping: &'a SourceMap,
}

impl<'a> StatementAnalysis<'a> {
    const fn new(
        statement: &'a Statement,
        bindings: &'a Bindings,
        source: &'a QuerySource,
        schema: &'a InitializedSchema,
        mapping: &'a SourceMap,
    ) -> Self {
        Self {
            statement,
            bindings,
            source,
            schema,
            mapping,
        }
    }

    fn run(
        &self,
        expand_asterisk: bool,
        analyze_access: bool,
        cache_context: &mut CacheContext,
    ) -> QueryResult {
        let base = self.schema.base_context();
        let mut result = QueryResult::for_statement(self.statement, self.mapping);
        let mut context =
            SchemaFileContext::new(&self.source.path, base, base.get_args(), base.metadata());
        context.set_diagnostic_source(self.statement_text());
        context.set_statement_span(*self.statement.span());
        let hints = self.bindings.type_hints();
        if let Err(error) = context.setup_typehinting(&hints) {
            result.report(self, QueryAnalysisStage::hint(), &error.to_string());
            return result;
        }
        match self.statement.deparse_sql(self.statement.dialect()) {
            Ok(sql) => result.sql = Some(sql),
            Err(error) => result.report(self, QueryAnalysisStage::deparse(), &error.to_string()),
        }
        if expand_asterisk && result.sql.is_none() && !analyze_access {
            return result;
        }
        // Imported catalog CTEs need one projection coordinate space. Keep the
        // original expression spans for diagnostics and query/parameter mapping.
        let mut output = match self.prepare_output(&result, expand_asterisk) {
            Ok(output) => output,
            Err(error) if analyze_access => {
                result.clear_output(self, QueryAnalysisStage::parse(), &error);
                None
            }
            Err(error) => return self.output_failure(result, QueryAnalysisStage::parse(), &error),
        };
        let statement = output.as_ref().unwrap_or(self.statement);
        match statement.analyze(context, cache_context) {
            Ok(pair) => self.complete(&mut result, pair, output.as_mut(), analyze_access),
            Err(error) => result.report(self, QueryAnalysisStage::analysis(), &error.to_string()),
        }
        result
    }

    fn statement_text(&self) -> &str {
        let span = self.statement.span();
        &self.mapping.rendered[span.offset()..span.offset() + span.len()]
    }

    fn prepare_output(
        &self,
        result: &QueryResult,
        enabled: bool,
    ) -> Result<Option<Statement>, String> {
        if !enabled {
            return Ok(None);
        }
        let Some(sql) = result.sql.as_deref() else {
            return Err("query could not be rendered for wildcard expansion".to_owned());
        };
        prepare_output_statement(self.statement, sql, self.mapping.expanded).map(Some)
    }

    fn complete(
        &self,
        result: &mut QueryResult,
        pair: ::sql_fun_sqlast::AstAndContextPair<SchemaFileContext>,
        output: Option<&mut Statement>,
        analyze_access: bool,
    ) {
        if let Some(accesses) = collect_requested_accesses(analyze_access, || {
            ::sql_fun_sqlast::sem::QueryAccess::from_statement(pair.context(), pair.ast())
        }) {
            match accesses {
                Ok(accesses) => result.describe_accesses(&accesses),
                Err(error) => {
                    result.report(self, QueryAnalysisStage::analysis(), &error.to_string());
                }
            }
        }
        if let Some(output) = output {
            let expansions = pair.context().analysis_state().wildcard_expansions();
            match rewrite_executable_sql(output, expansions) {
                Ok(sql) => result.sql = Some(sql),
                Err(error) => result.clear_output(self, QueryAnalysisStage::deparse(), &error),
            }
        }
        result.describe(
            pair,
            self.bindings,
            self.source,
            self.statement,
            self.mapping,
        );
    }

    fn output_failure(
        &self,
        mut result: QueryResult,
        stage: QueryAnalysisStage,
        error: &str,
    ) -> QueryResult {
        result.clear_output(self, stage, error);
        result
    }

    fn diagnostic(&self, stage: QueryAnalysisStage, error: &str) -> AnalysisProbrem {
        let source = if self.mapping.expanded {
            None
        } else {
            Some(self.statement_text())
        };
        diagnostic(
            &AnalysisProblem::query_analysis(stage, error),
            &self.source.path,
            source,
            self.schema.base_context().metadata().sql_dialect(),
            self.statement.span().offset(),
        )
    }
}

/// Preserve diagnostic anchors while giving imported projections one coordinate space.
fn prepare_output_statement(
    statement: &Statement,
    sql: &str,
    expanded: bool,
) -> Result<Statement, String> {
    let mut output = statement.clone();
    if expanded {
        let parsed = parse_realized_statement(sql, statement.dialect())?;
        output
            .synchronize_target_locations(parsed)
            .map_err(|error| error.to_string())?;
    }
    Ok(output)
}

/// Render executable SQL only after the checked mutation has completed.
fn rewrite_executable_sql(
    statement: &mut Statement,
    expansions: &[::sql_fun_sqlast::sem::WildcardExpansion],
) -> Result<String, String> {
    statement
        .rewrite_wildcards(expansions)
        .map_err(|error| error.to_string())?;
    statement
        .deparse_sql(statement.dialect())
        .map_err(|error| error.to_string())
}

/// Re-establish parser locations after structural catalog CTE insertion.
fn parse_realized_statement(sql: &str, dialect: Dialect) -> Result<Statement, String> {
    let mut statements = parse_statements(sql, dialect);
    let Some(statement) = statements.next() else {
        return Err("deparsed query has no statement".to_owned());
    };
    let statement = statement.map_err(|error| error.to_string())?;
    if statements.next().is_some() {
        return Err("deparsed query must contain exactly one statement".to_owned());
    }
    Ok(statement)
}

impl QueryResult {
    fn describe(
        &mut self,
        pair: ::sql_fun_sqlast::AstAndContextPair<SchemaFileContext>,
        bindings: &Bindings,
        source: &QuerySource,
        statement: &Statement,
        mapping: &SourceMap,
    ) {
        let dialect = pair.context().metadata().sql_dialect();
        let text = &mapping.rendered
            [statement.span().offset()..statement.span().offset() + statement.span().len()];
        let diagnostic_source = if mapping.expanded { None } else { Some(text) };

        self.describe_columns(pair.ast());
        let mut context = pair.into_context();
        if let Err(error) = QueryMetadata::report_unresolved_parameters(&mut context) {
            self.problems.push(diagnostic(
                &AnalysisProblem::query_analysis(
                    QueryAnalysisStage::analysis(),
                    &error.to_string(),
                ),
                &source.path,
                diagnostic_source,
                dialect,
                statement.span().offset(),
            ));
        }
        let mut typed = true;
        for parameter in context.parameter_constraints().iter() {
            let ty = optional_named_type(parameter.constraint().single_type().as_ref());
            typed &= ty.is_some();
            self.parameters.push(ParameterDescribe::new(
                &bindings.name(parameter.position()),
                parameter.position(),
                ty,
                mapping.parameter_spans(statement, parameter.position()),
            ));
        }
        for (index, problem) in context.problems().iter().enumerate() {
            if mapping.expanded {
                self.problems.push(
                    AnalysisProbrem::new(
                        problem.code(),
                        DiagnosticSeverity::from(problem.severity()),
                        &source.path,
                        None,
                    )
                    .with_semantic_problem(problem),
                );
            } else {
                let span = problem
                    .statement_relative_span()
                    .map(|span| StatementByteRange::new(span.offset(), span.len()));
                self.problems.push(
                    AnalysisProbrem::new(
                        problem.code(),
                        DiagnosticSeverity::from(problem.severity()),
                        &source.path,
                        span,
                    )
                    .with_presentation(context.diagnostic_presentations()[index].as_ref()),
                );
            }
        }
        self.complete = typed
            && self.sql.is_some()
            && !context.problems().iter().any(|problem| {
                problem.severity() == ::sql_fun_core::analysis_problem::DiagnosticSeverity::Error
            })
            && self.problems.len() == context.problems().len();
    }

    fn describe_columns(&mut self, ast: &::sql_fun_sqlast::sem::SemAst) {
        if let Some(columns) = ast.result_columns() {
            for ordinal in 0..columns.len() {
                let column = columns
                    .get_column_at(ordinal)
                    .expect("column within length");
                self.columns.push(ResultSetColumn::new(
                    u32::try_from(ordinal).expect("SQL result ordinal fits u32"),
                    &column
                        .column_name()
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                    optional_named_type(column.col_type().as_ref()),
                    Nullability::from_not_null(*column.is_not_null()),
                ));
            }
        }
    }

    fn describe_accesses(&mut self, accesses: &::sql_fun_sqlast::sem::QueryAccess) {
        for access in accesses.records() {
            let object = if access.relation_kind().is_table() {
                ObjectKind::table()
            } else if access.relation_kind().is_materialized_view() {
                ObjectKind::materialized_view()
            } else if access.relation_kind().is_view() {
                ObjectKind::view()
            } else {
                ObjectKind::function()
            };
            let name = access.relation();
            let relation = RelationName::new(
                name.catalog()
                    .map(::sql_fun_sqlast::names::CatalogName::as_str),
                name.schema()
                    .map(::sql_fun_sqlast::names::SchemaName::as_str),
                name.name().as_str(),
            );
            self.accesses.push(ColumnAccess::new(
                *access.access_kind(),
                object,
                &relation,
                access
                    .column_name()
                    .as_ref()
                    .map(::sql_fun_sqlast::names::ColumnName::as_str),
            ));
        }
    }
}

fn optional_named_type(ty: Option<&SqlType>) -> Option<TypeReference> {
    if let Some(ty) = ty {
        named_type(ty)
    } else {
        None
    }
}

fn named_type(ty: &SqlType) -> Option<TypeReference> {
    // Dynamic anonymous definitions have no named API type; never call full_name on one.
    if ty.type_definition().is_some() {
        return None;
    }
    let name = ty.full_name()?;
    Some(TypeReference::new(
        name.schema().map_or("", |schema| schema.as_str()),
        &name.local_name(),
        ty.is_array() == Some(true),
    ))
}

const fn query_kind(statement: &Statement) -> QueryKind {
    classify_kind(Some(statement.kind()))
}

const fn classify_kind(kind: Option<::sql_fun_sqlast::ast::StatementKind>) -> QueryKind {
    use ::sql_fun_sqlast::ast::StatementKind;
    match kind {
        Some(StatementKind::Query) => QueryKind::Select,
        Some(StatementKind::Insert) => QueryKind::Insert,
        Some(StatementKind::Update) => QueryKind::Update,
        Some(StatementKind::Delete) => QueryKind::Delete,
        Some(StatementKind::Merge) => QueryKind::Merge,
        Some(StatementKind::Truncate) => QueryKind::Truncate,
        _ => QueryKind::Other,
    }
}

fn diagnostic(
    problem: &AnalysisProblem,
    path: &Path,
    source: Option<&str>,
    dialect: ::sql_fun_core::SqlDialect,
    offset: usize,
) -> AnalysisProbrem {
    let span = problem
        .statement_relative_span()
        .map(|span| StatementByteRange::new(span.offset(), span.len()));
    if let Some(source) = source {
        let presentation = DiagnosticPresentation::new(problem, source, dialect, offset);
        AnalysisProbrem::new(
            problem.code(),
            DiagnosticSeverity::from(problem.severity()),
            path,
            span,
        )
        .with_presentation(Some(&presentation))
    } else {
        AnalysisProbrem::new(
            problem.code(),
            DiagnosticSeverity::from(problem.severity()),
            path,
            None,
        )
        .with_semantic_problem(problem)
    }
}

fn analyze(
    args: &AnalyzeQueryArgs,
    source: &QuerySource,
    schema: &InitializedSchema,
    expand_asterisk: bool,
    cache_context: &mut CacheContext,
) -> Vec<QueryResult> {
    let dialect = Dialect::from(schema.base_context().metadata());
    let fail = |stage: QueryAnalysisStage, message: &str| {
        vec![QueryResult::failure(
            stage,
            message,
            source,
            schema.base_context().metadata().sql_dialect(),
        )]
    };
    let query = PlaceholderedSql::from_query(&source.text);
    if let Some(catalog) = args.resolved_cte_catalog() {
        let ctes = catalog
            .try_map_cte_queries(|_, _| Ok::<_, std::convert::Infallible>(()))
            .expect("infallible inspection");
        if !ctes.is_empty() {
            return match crate::cte_realization::realize_cte_catalog(&query, catalog, dialect) {
                Ok(realized) => {
                    let bindings = Bindings::from_realized(&realized);
                    if let Err(error) =
                        catalog.try_map_cte_queries(|_, query| bindings.render(query, dialect))
                    {
                        return fail(QueryAnalysisStage::realization(), &error);
                    }
                    // Spans of the realized AST refer to the rendered base input.
                    match bindings.spans(&query, dialect) {
                        Ok(mut mapping) => {
                            mapping.expanded = true;
                            vec![
                                StatementAnalysis::new(
                                    realized.statement(),
                                    &bindings,
                                    source,
                                    schema,
                                    &mapping,
                                )
                                .run(
                                    expand_asterisk,
                                    *args.analyze_access(),
                                    cache_context,
                                ),
                            ]
                        }
                        Err(error) => fail(QueryAnalysisStage::realization(), &error),
                    }
                }
                Err(error) => fail(QueryAnalysisStage::realization(), &error.to_string()),
            };
        }
    }
    let bindings = match Bindings::from_query(&query) {
        Ok(bindings) => bindings,
        Err(error) => {
            return fail(QueryAnalysisStage::hint(), &error);
        }
    };
    let mapping = match bindings.spans(&query, dialect) {
        Ok(mapping) => mapping,
        Err(error) => {
            return fail(QueryAnalysisStage::realization(), &error);
        }
    };
    parse_statements(&mapping.rendered, dialect)
        .map(|statement| match statement {
            Ok(statement) => StatementAnalysis::new(
                &statement, &bindings, source, schema, &mapping,
            )
            .run(expand_asterisk, *args.analyze_access(), cache_context),
            Err(error) => QueryResult::parse_failure(&error, source, schema, &mapping),
        })
        .collect()
}

impl Session<'_, '_> {
    pub(super) fn analyze_query(
        &mut self,
        args: &AnalyzeQueryArgs,
    ) -> Result<AnalyzeQueryResponse, SqlFunServerApiError> {
        let schema = self
            .context(args.context_handle())
            .ok_or_else(SqlFunServerApiError::invalid_context_handle)?;
        let source = QuerySource::from_request(args)?;
        let results = analyze(
            args,
            &source,
            schema,
            self.expand_asterisk(args.context_handle()),
            self.cache_context(args.context_handle()),
        );
        let mut handles = Vec::new();
        let response = (|| {
            let mut items = Vec::new();
            for result in results {
                items.push(AnalyzeQueryItem::new(
                    result.span,
                    result.kind,
                    result.sql,
                    result.complete,
                    self.query_page(result.problems, &mut handles)?,
                    self.query_page(result.parameters, &mut handles)?,
                    self.query_page(result.columns, &mut handles)?,
                    self.query_page(result.accesses, &mut handles)?,
                ));
            }
            Ok(AnalyzeQueryResponse::new(
                self.query_page(items, &mut handles)?,
            ))
        })();
        if response.is_err() {
            for handle in handles {
                self.release_collection(
                    &::sql_fun_server_api::payloads::ReleaseCollectionArgs::new(&handle),
                )?;
            }
        }
        response
    }

    fn query_page<T: ::serde::Serialize + ::serde::de::DeserializeOwned>(
        &mut self,
        items: Vec<T>,
        handles: &mut Vec<CollectionHandle>,
    ) -> Result<Collection<T>, SqlFunServerApiError> {
        // The registry owns immutable JSON items; nested handles are assigned exactly once.
        let values = items
            .into_iter()
            .map(::serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| SqlFunServerApiError::new_unexpected(&error.to_string()))?;
        let handle = self.register_collection(values, Vec::len, Clone::clone)?;
        handles.push(handle.clone());
        let page = self.read_collection(
            &::sql_fun_server_api::payloads::ReadCollectionArgs::new(&handle, 0, None),
        )?;
        ::serde_json::from_value(page)
            .map_err(|error| SqlFunServerApiError::new_unexpected(&error.to_string()))
    }
}

// Keep the actual traversal behind request intent, including its allocation cost.
fn collect_requested_accesses(
    requested: bool,
    collect: impl FnOnce() -> Result<
        ::sql_fun_sqlast::sem::QueryAccess,
        ::sql_fun_sqlast::sem::AnalysisError,
    >,
) -> Option<Result<::sql_fun_sqlast::sem::QueryAccess, ::sql_fun_sqlast::sem::AnalysisError>> {
    if requested { Some(collect()) } else { None }
}

#[cfg(test)]
mod test_access_invocation {
    #[test]
    fn default_and_false_never_invoke_the_collector() {
        let omitted = ::serde_json::json!({"context_handle":"c", "query":{"Text":"SELECT 1"}});
        for option in [None, Some(false), Some(true)] {
            let mut value = omitted.clone();
            if let Some(option) = option {
                value["analyze_access"] = option.into();
            }
            let args: ::sql_fun_server_api::payloads::AnalyzeQueryArgs =
                ::serde_json::from_value(value).unwrap();
            let calls = std::cell::Cell::new(0);
            let result = super::collect_requested_accesses(*args.analyze_access(), || {
                calls.set(calls.get() + 1);
                Ok(::sql_fun_sqlast::sem::QueryAccess::default())
            });
            assert_eq!(result.is_some(), option == Some(true));
            assert_eq!(calls.get(), usize::from(option == Some(true)));
        }
    }
}
