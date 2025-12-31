use crate::{
    AstAndContextPair,
    sem::{
        AnalysisError, ParseContext, analyze_alter_owner, analyze_alter_seq,
        analyze_alter_statement, analyze_comment, analyze_create_comp_type, analyze_create_domain,
        analyze_create_enum_type, analyze_create_extension, analyze_create_function,
        analyze_create_schema, analyze_create_seq, analyze_create_stmt, analyze_create_table_as,
        analyze_create_view, analyze_index_stmt, analyze_select, analyze_variable_set,
    },
    syn::{ListOpt, Opt, ParseResult},
};

/// analyze top-level SQL statement
pub fn semantic_analysis<TParseContext>(
    context: TParseContext,
    syn: ParseResult,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    let statement = syn.get_stmts().get(0).get_stmt();
    analyze_node(context, &None, statement)
}

/// analyze node
#[tracing::instrument(skip(context))]
pub fn analyze_node<TParseContext: ParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    statement: crate::syn::Node,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError> {
    if let Some(create) = statement.as_create_stmt().as_inner() {
        return analyze_create_stmt(context, parent_schema, create);
    }

    if let Some(create) = statement.as_create_enum_stmt().as_inner() {
        return analyze_create_enum_type(context, parent_schema, create);
    }

    if let Some(comp_type) = statement.as_composite_type_stmt().as_inner() {
        return analyze_create_comp_type(context, parent_schema, comp_type);
    }

    if let Some(create_extension) = statement.as_create_extension_stmt().as_inner() {
        return analyze_create_extension(context, parent_schema, create_extension);
    }

    if let Some(create_function) = statement.as_create_function_stmt().as_inner() {
        return analyze_create_function(context, parent_schema, create_function);
    }

    if let Some(variable_set) = statement.as_variable_set_stmt().as_inner() {
        return analyze_variable_set(context, variable_set);
    }

    if let Some(select) = statement.as_select_stmt().as_inner() {
        return analyze_select(context, select);
    }

    if let Some(create_schema) = statement.as_create_schema_stmt().as_inner() {
        return analyze_create_schema(context, create_schema);
    }

    if let Some(alter_owner) = statement.as_alter_owner_stmt().as_inner() {
        return analyze_alter_owner(context, alter_owner);
    }

    if let Some(comment) = statement.as_comment_stmt().as_inner() {
        return analyze_comment(context, comment);
    }

    if let Some(domain) = statement.as_create_domain_stmt().as_inner() {
        return analyze_create_domain(context, domain);
    }

    if let Some(alter_table) = statement.as_alter_table_stmt().as_inner() {
        return analyze_alter_statement(context, parent_schema, alter_table);
    }

    if let Some(create_view) = statement.as_view_stmt().as_inner() {
        return analyze_create_view(context, parent_schema, create_view);
    }

    if let Some(create_seq) = statement.as_create_seq_stmt().as_inner() {
        return analyze_create_seq(context, parent_schema, create_seq);
    }

    if let Some(alter_seq) = statement.as_alter_seq_stmt().as_inner() {
        return analyze_alter_seq(context, parent_schema, alter_seq);
    }

    if let Some(create_table_as) = statement.as_create_table_as_stmt().as_inner() {
        return analyze_create_table_as(context, parent_schema, create_table_as);
    }

    if let Some(index_stmt) = statement.as_index_stmt().as_inner() {
        return analyze_index_stmt(context, parent_schema, index_stmt);
    }

    todo!("handle {statement:?}");
}

#[cfg(test)]
mod tests {
    use crate::{
        AstAndContextPair,
        sem::{BaseContext, SchemaFileContext, semantic_analysis},
        tests::context_args,
    };

    use sql_fun_core::SqlFunArgs;
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
    ) -> TestResult {
        let parsed = crate::parse(sql)?;
        eprintln!("{parsed:?}");

        let base_context = BaseContext::new(context_args)?;

        let file_context = SchemaFileContext::new(base_context);
        let AstAndContextPair(_ast, _context) = semantic_analysis(file_context, parsed)?;

        Ok(())
    }
}
