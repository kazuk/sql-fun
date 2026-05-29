use sql_fun_core::IVec;

use crate::{
    AstAndContextPair,
    sem::{
        AlterOwner, AlterSequence, AlterTable, AnalysisError, Comment, CreateCompositType,
        CreateDomain, CreateEnum, CreateExtension, CreateFunction, CreateIndex, CreateSchema,
        CreateSequence, CreateTable, CreateTableAs, CreateView, ParseContext, SelectStatement,
        SemAst, VariableSet, create_table::DropTable,
    },
    syn::{ListOpt, Opt, ParseResult},
};

impl SemAst {
    /// analyze top-level SQL statement
    pub fn semantic_analysis<TParseContext>(
        context: TParseContext,
        syn: ParseResult,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let statement = syn.get_stmts().get(0).get_stmt();
        Self::analyze_node(context, &None, statement, tokens)
    }
}

/// analyze top-level SQL statement
#[deprecated = "use SemAst::semantic_analysis"]
pub fn semantic_analysis<TParseContext>(
    context: TParseContext,
    syn: ParseResult,
    tokens: &IVec<crate::syn::ScanToken>,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    SemAst::semantic_analysis(context, syn, tokens)
}

impl SemAst {
    /// analyze drop object statement
    pub fn analyze_drop_statement<TParseContext>(
        context: TParseContext,
        statement: crate::syn::DropStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(drop_type) = statement.get_remove_type().as_inner() else {
            AnalysisError::raise_unexpected_none("DropStmt.remove_type")?;
            AnalysisError::never()
        };

        match drop_type {
            crate::syn::ObjectType::ObjectTable => {
                DropTable::analyze_drop_stmt(context, statement, tokens)
            }
            _ => {
                AnalysisError::raise_unexpected_input(&format!("{drop_type:?}"))?;
                AnalysisError::never()
            }
        }
    }

    /// analyze node
    #[tracing::instrument(skip(context))]
    pub fn analyze_node<TParseContext: ParseContext>(
        context: TParseContext,
        parent_schema: &Option<String>,
        statement: crate::syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError> {
        let Some(source_span) = context.get_source_path_with_span().cloned() else {
            AnalysisError::raise_unexpected_none("context get_source_path_with_span")?;
            AnalysisError::never();
        };

        if let Some(create) = statement.as_create_stmt().as_inner() {
            return CreateTable::analyze_create_stmt(
                context,
                parent_schema,
                create,
                tokens,
                &source_span,
            );
        }

        if let Some(drop) = statement.as_drop_stmt().as_inner() {
            return Self::analyze_drop_statement(context, drop, tokens);
        }

        if let Some(create) = statement.as_create_enum_stmt().as_inner() {
            return CreateEnum::analyze_create_enum_type(context, parent_schema, create);
        }

        if let Some(comp_type) = statement.as_composite_type_stmt().as_inner() {
            return CreateCompositType::analyze_create_comp_type(context, parent_schema, comp_type);
        }

        if let Some(create_extension) = statement.as_create_extension_stmt().as_inner() {
            return CreateExtension::analyze_create_extension(
                context,
                parent_schema,
                create_extension,
            );
        }

        if let Some(create_function) = statement.as_create_function_stmt().as_inner() {
            return CreateFunction::analyze_create_function(
                context,
                parent_schema,
                create_function,
                tokens,
            );
        }

        if let Some(variable_set) = statement.as_variable_set_stmt().as_inner() {
            return VariableSet::analyze_variable_set(context, &variable_set, tokens);
        }

        if let Some(select) = statement.as_select_stmt().as_inner() {
            return SelectStatement::analyze_select(context, select, tokens);
        }

        if let Some(create_schema) = statement.as_create_schema_stmt().as_inner() {
            return CreateSchema::analyze_create_schema(context, create_schema, tokens);
        }

        if let Some(alter_owner) = statement.as_alter_owner_stmt().as_inner() {
            return AlterOwner::analyze_alter_owner(context, alter_owner);
        }

        if let Some(comment) = statement.as_comment_stmt().as_inner() {
            return Comment::analyze_comment(context, comment);
        }

        if let Some(domain) = statement.as_create_domain_stmt().as_inner() {
            return CreateDomain::analyze_create_domain(context, domain, tokens);
        }

        if let Some(alter_table) = statement.as_alter_table_stmt().as_inner() {
            return AlterTable::analyze_alter_statement(
                context,
                parent_schema,
                alter_table,
                tokens,
                &source_span,
            );
        }

        if let Some(create_view) = statement.as_view_stmt().as_inner() {
            return CreateView::analyze_create_view(context, parent_schema, create_view, tokens);
        }

        if let Some(create_seq) = statement.as_create_seq_stmt().as_inner() {
            return CreateSequence::analyze_create_seq(context, parent_schema, create_seq);
        }

        if let Some(alter_seq) = statement.as_alter_seq_stmt().as_inner() {
            return AlterSequence::analyze_alter_seq(context, parent_schema, alter_seq);
        }

        if let Some(create_table_as) = statement.as_create_table_as_stmt().as_inner() {
            return CreateTableAs::analyze_create_table_as(
                context,
                parent_schema,
                create_table_as,
                tokens,
                &source_span,
            );
        }

        if let Some(index_stmt) = statement.as_index_stmt().as_inner() {
            return CreateIndex::analyze_index_stmt(context, parent_schema, index_stmt, tokens);
        }

        todo!("handle {statement:?}");
    }
}

/// analyze node
#[deprecated = "use SemAst::analyze_node"]
pub fn analyze_node<TParseContext: ParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    statement: crate::syn::Node,
    tokens: &IVec<crate::syn::ScanToken>,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError> {
    SemAst::analyze_node(context, parent_schema, statement, tokens)
}

#[cfg(test)]
mod tests {
    use crate::{
        AstAndContextPair, PostgresDialect,
        sem::{BaseContext, MutSourcePathWithSpan, SchemaFileContext, SourcePathWithSpan},
        test_helpers::source_span,
        tests::context_args,
    };

    use sql_fun_core::SqlFunArgs;
    use std::path::Path;
    use testresult::TestResult;

    #[rstest::rstest]
    #[case(r#"create table users ( id bigserial not null primary key, name varchar )"#)]
    #[case(r#"create type user_kind as enum('member', 'guest')"#)]
    #[case(r#"CREATE TYPE compfoo AS (f1 int, f2 text)"#)]
    #[case(r#"CREATE EXTENSION hstore;"#)]
    #[case(
        r#"CREATE FUNCTION hstore_subscript_handler(internal)
RETURNS internal
AS 'MODULE_PATHNAME', 'hstore_subscript_handler'
LANGUAGE C STRICT IMMUTABLE PARALLEL SAFE;"#
    )]
    #[case("SET statement_timeout = 0;")]
    #[case(r#"CREATE DOMAIN public."AccountNumber" AS character varying(15);"#)]
    fn test_schema_file_context_accept_ddl(
        #[case] sql: &str,
        context_args: SqlFunArgs,
        source_span: SourcePathWithSpan,
    ) -> TestResult {
        use crate::sem::SemAst;

        let parsed = PostgresDialect::parse(sql)?;
        let tokens = PostgresDialect::scan(sql)?;

        eprintln!("{parsed:?}");

        let base_context = BaseContext::new(context_args)?;
        let path = Path::new("test.sql");
        let mut file_context = SchemaFileContext::new(&path, base_context);
        file_context.set_source_path_with_span(&source_span);
        let AstAndContextPair(_ast, _context) =
            SemAst::semantic_analysis(file_context, parsed, &tokens)?;

        Ok(())
    }
}
