use crate::{
    AstAndContextPair,
    sem::{
        AlterOwner, AlterSequence, AlterTable, AnalysisError, Comment, CreateCompositeType,
        CreateDomain, CreateEnum, CreateFunction, CreateIndex, CreateSchema, CreateSequence,
        CreateTable, CreateTableAs, CreateView, ParseContext, SemAst, create_table::DropTable,
        unresolved::TryFromWithTokens,
    },
    syn::{ListOpt, Opt, ParseResult},
};
#[cfg(feature = "sqlite")]
use crate::{SqliteToken, syn::sqlite::Stmt};

use ::sql_fun_core::IVec;

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
            return CreateCompositeType::analyze_create_comp_type(
                context,
                parent_schema,
                comp_type,
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

        if statement.as_select_stmt().as_inner().is_some()
            || statement.as_insert_stmt().as_inner().is_some()
            || statement.as_update_stmt().as_inner().is_some()
            || statement.as_delete_stmt().as_inner().is_some()
            || statement.as_view_stmt().as_inner().is_some()
            || statement.as_create_extension_stmt().as_inner().is_some()
            || statement.as_variable_set_stmt().as_inner().is_some()
        {
            let syntax =
                crate::sem::unresolved::Statement::try_from_with_tokens(&statement, tokens)?;
            return syntax.resolve_and_apply(context, crate::sem::unresolved::Dialect::postgres());
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

    /// analyze [`Stmt`]
    #[cfg(feature = "sqlite")]
    pub fn semantic_analysis_sqlite_stmt<TContext>(
        context: TContext,
        syn: Stmt,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let Some(source_span) = context.get_source_path_with_span().cloned() else {
            AnalysisError::raise_unexpected_none("context get_source_path_with_span")?;
            AnalysisError::never();
        };

        match syn {
            Stmt::AlterTable(alter_table) => {
                AlterTable::analyze_alter_table(context, alter_table, tokens, &source_span)
            }
            Stmt::CreateTable(create_table_statement) => CreateTable::analyze_create_table(
                context,
                &None,
                create_table_statement,
                tokens,
                &source_span,
            ),
            Stmt::DropTable(_drop_table_statement) => todo!(),
            Stmt::Analyze(_analyze_statement) => todo!(),
            Stmt::Attach(_attach_statement) => todo!(),
            Stmt::Detach(_detach_statement) => todo!(),
            Stmt::Vacuum(_vacuum_statement) => todo!(),
            Stmt::Begin(_begin_statement) => todo!(),
            Stmt::Commit(_commit_statement) => todo!(),
            Stmt::Release(_release_statement) => todo!(),
            Stmt::Rollback(_rollback_statement) => todo!(),
            Stmt::Savepoint(_savepoint_statement) => todo!(),
            Stmt::CreateIndex(_create_index_statement) => todo!(),
            Stmt::DropIndex(_drop_index_statement) => todo!(),
            Stmt::Reindex(_reindex_statement) => todo!(),
            Stmt::Optimize(_optimize_statement) => todo!(),
            Stmt::CreateVirtualTable(_create_virtual_table) => todo!(),
            Stmt::CreateTrigger(_create_trigger_statement) => todo!(),
            Stmt::DropTrigger(_drop_trigger_statement) => todo!(),
            Stmt::CreateView(view) => {
                let syntax = crate::sem::unresolved::Statement::try_from(&Stmt::CreateView(view))?;
                syntax.resolve_and_apply(context, crate::sem::unresolved::Dialect::sqlite())
            }
            Stmt::CreateMaterializedView(_create_materialized_view) => todo!(),
            Stmt::DropView(_drop_view) => todo!(),
            Stmt::CreateType(_create_type_statement) => todo!(),
            Stmt::DropType(_drop_type_statement) => todo!(),
            Stmt::CreateSequence(_create_sequence_statement) => todo!(),
            Stmt::DropSequence(_drop_sequence_statement) => todo!(),
            Stmt::Select(select) => {
                let syntax = crate::sem::unresolved::Statement::try_from(&Stmt::Select(select))?;
                syntax.resolve_and_apply(context, crate::sem::unresolved::Dialect::sqlite())
            }
            Stmt::Update(_update) => todo!(),
            Stmt::Delete(_delete_statement) => todo!(),
            Stmt::Insert(_insert_statement) => todo!(),
            Stmt::Pragma(_pragma_statement) => todo!(),
            Stmt::CreateDomain(_create_domain_statement) => todo!(),
            Stmt::DropDomain(_drop_domain_statement) => todo!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        AstAndContextPair, PostgresDialect,
        sem::{BaseContext, MutSourcePathWithSpan, SchemaFileContext, SourcePathWithSpan},
        test_helpers::source_span,
        tests::context_args,
    };

    use std::path::Path;

    use ::sql_fun_core::SqlFunArgs;
    use ::testresult::TestResult;

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
    #[case("CREATE VIEW users_view AS SELECT 1 AS id;")]
    #[case("insert into users (id) values (1)")]
    #[case("update users set id = 1")]
    #[case("delete from users")]
    #[case("with changed as (delete from users returning id) select 1")]
    fn test_schema_file_context_accepts_top_level_statements(
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
        let mut file_context = SchemaFileContext::new(path, base_context);
        file_context.set_source_path_with_span(&source_span);
        let AstAndContextPair(_ast, _context) =
            SemAst::semantic_analysis(file_context, parsed, &tokens)?;

        Ok(())
    }
}
