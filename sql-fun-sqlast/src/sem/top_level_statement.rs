use crate::{
    AstAndContextPair,
    ast::TryFromWithTokens,
    sem::{AnalysisError, ParseContext, SemAst, create_table::DropTable},
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
        _parent_schema: &Option<String>,
        statement: crate::syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError> {
        AnalysisError::check_dialect(crate::ast::Dialect::postgres(), context.dialect())?;
        let Some(_) = context.get_source_path_with_span() else {
            AnalysisError::raise_unexpected_none("context get_source_path_with_span")?;
            AnalysisError::never();
        };

        let syntax = crate::ast::StatementNode::try_from_with_tokens(&statement, tokens)?;
        syntax.resolve_and_apply(context)
    }

    /// analyze [`Stmt`]
    #[cfg(feature = "sqlite")]
    pub fn semantic_analysis_sqlite_stmt<TContext>(
        context: TContext,
        syn: Stmt,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let Some(_source_span) = context.get_source_path_with_span().cloned() else {
            AnalysisError::raise_unexpected_none("context get_source_path_with_span")?;
            AnalysisError::never();
        };

        AnalysisError::check_dialect(crate::ast::Dialect::sqlite(), context.dialect())?;
        match syn {
            Stmt::AlterTable(alter_table) => {
                let syntax = crate::ast::StatementNode::try_from(&Stmt::AlterTable(alter_table))?;
                syntax.resolve_and_apply(context)
            }
            Stmt::CreateTable(create_table_statement) => {
                let syntax = crate::ast::StatementNode::try_from(&Stmt::CreateTable(
                    create_table_statement,
                ))?;
                syntax.resolve_and_apply(context)
            }
            Stmt::DropTable(drop_table_statement) => {
                let syntax =
                    crate::ast::StatementNode::try_from(&Stmt::DropTable(drop_table_statement))?;
                syntax.resolve_and_apply(context)
            }
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
                let syntax = crate::ast::StatementNode::try_from(&Stmt::CreateView(view))?;
                syntax.resolve_and_apply(context)
            }
            Stmt::CreateMaterializedView(_create_materialized_view) => todo!(),
            Stmt::DropView(_drop_view) => todo!(),
            Stmt::CreateType(_create_type_statement) => todo!(),
            Stmt::DropType(_drop_type_statement) => todo!(),
            Stmt::CreateSequence(_create_sequence_statement) => todo!(),
            Stmt::DropSequence(_drop_sequence_statement) => todo!(),
            Stmt::Select(select) => {
                let syntax = crate::ast::StatementNode::try_from(&Stmt::Select(select))?;
                syntax.resolve_and_apply(context)
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
        sem::{
            BaseContext, FrozenBaseContext, MutSourcePathWithSpan, SchemaFileContext,
            SourcePathWithSpan,
        },
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
    #[case("ALTER TYPE public.user_kind OWNER TO application_owner;")]
    #[case("CREATE VIEW users_view AS SELECT 1 AS id;")]
    #[case("CREATE SCHEMA app CREATE TABLE app.users (id integer);")]
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

        let collection = context_args.load_metadata()?;
        let selected = collection.get("default").expect("selected test metadata");
        let base_context = BaseContext::new(&context_args, selected)?;
        let path = Path::new("test.sql");
        let mut file_context = {
            let analysis_base = &FrozenBaseContext::from(base_context);
            SchemaFileContext::new(path, analysis_base, &context_args, selected)
        };
        file_context.set_source_path_with_span(&source_span);
        let AstAndContextPair(_ast, _context) =
            SemAst::semantic_analysis(file_context, parsed, &tokens)?;

        Ok(())
    }
}
