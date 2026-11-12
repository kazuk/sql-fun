//! test helper types
#![cfg(test)]

pub(crate) mod alter_column_type;
pub(crate) mod alter_index;
pub(crate) mod alter_syntax;
#[cfg(feature = "sqlite")]
pub(crate) mod alter_table;
pub(crate) mod analysis_cache;
pub(crate) mod analysis_lifecycle;
pub(crate) mod ast_query;
pub(crate) mod between;
pub(crate) mod builtin_functions;
pub(crate) mod call_arguments;
pub(crate) mod case_parameters;
pub(crate) mod cast_catalog;
pub(crate) mod cast_layers;
pub(crate) mod checks;
pub(crate) mod coalesce;
pub(crate) mod collate;
pub(crate) mod column_attributes;
#[cfg(feature = "sqlite")]
pub(crate) mod column_definition;
pub(crate) mod column_rename;
pub(crate) mod common_types;
pub(crate) mod composite_domains;
pub(crate) mod conflict_masters;
pub(crate) mod counted_root;
pub(crate) mod data_source_table;
pub(crate) mod declaration_collation;
pub(crate) mod delete;
pub(crate) mod delete_sql_driven;
pub(crate) mod delete_versions;
pub(crate) mod deparse;
pub(crate) mod deparse_constraints;
pub(crate) mod deparse_lexical;
pub(crate) mod deparse_statements;
pub(crate) mod distinct;
pub(crate) mod document_merge;
pub(crate) mod domain_casts;
pub(crate) mod drop_rule;
pub(crate) mod drop_rule_syntax;
pub(crate) mod drop_syntax;
pub(crate) mod drop_trigger;
#[cfg(feature = "sqlite")]
pub(crate) mod drop_trigger_sqlite;
pub(crate) mod drop_trigger_syntax;
pub(crate) mod dynamic_types;
pub(crate) mod foreign_keys;
pub(crate) mod freeze;
pub(crate) mod function_overloads;
pub(crate) mod function_syntax;
pub(crate) mod grouping;
pub(crate) mod heavy;
pub(crate) mod index_layers;
pub(crate) mod indirection;
pub(crate) mod insert;
pub(crate) mod insert_join_and_key_semantics;
pub(crate) mod insert_views;
pub(crate) mod membership;
pub(crate) mod merge_assignments;
pub(crate) mod merge_statements;
pub(crate) mod merge_views;
mod name_builders;
pub(crate) mod name_identity;
pub(crate) mod notify;
pub(crate) mod operator_layers;
pub(crate) mod operator_names;
pub(crate) mod ownership;
pub(crate) mod parameter_calls;
pub(crate) mod pattern;
mod postgres_ast;
pub(crate) mod public_analysis;
pub(crate) mod quantified;
pub(crate) mod quantified_all;
pub(crate) mod quantified_any;
pub(crate) mod query;
pub(crate) mod query_access;
pub(crate) mod query_array_subquery;
pub(crate) mod query_source_namespace;
pub(crate) mod query_whole_rows;
pub(crate) mod rows_from;
pub(crate) mod rule;
pub(crate) mod sampling;
pub(crate) mod scalar_catalog;
pub(crate) mod schema_content_hash;
pub(crate) mod schema_creation;
pub(crate) mod search_path;
pub(crate) mod selected_metadata;
pub(crate) mod serial;
pub(crate) mod set_operations;
pub(crate) mod source_syntax;
#[cfg(feature = "sqlite")]
pub(crate) mod sqlite_from_groups;
#[cfg(feature = "sqlite")]
pub(crate) mod sqlite_names;
#[cfg(all(feature = "sqlite", feature = "workaround_turso_9791"))]
pub(crate) mod sqlite_retention;
pub(crate) mod stack_regressions;
pub(crate) mod statements;
pub(crate) mod structural;
mod syn_builder;
pub(crate) mod table_clone;
pub(crate) mod table_rename;
mod test_parse_context;
pub(crate) mod trigger;
pub(crate) mod truth_test;
pub(crate) mod type_catalog;
pub(crate) mod type_hint;
pub(crate) mod type_syntax;
pub(crate) mod unqualified_tables;
pub(crate) mod update;
pub(crate) mod update_assignments;
pub(crate) mod wildcard;
pub(crate) mod wildcard_rows_source;

pub(crate) use self::postgres_ast::{
    assert_postgres_structure, postgres_expression, postgres_node,
};
#[cfg(feature = "sqlite")]
pub use self::sqlite_syn_builder::{SqliteArrayTypeGate, SqliteSynBuilder, sqlite_syn};
pub use self::{
    syn_builder::SynBuilder,
    test_parse_context::{TestParseContext, test_context},
};
use crate::{
    CacheContext,
    context::{FrozenBaseContext, SchemaFileContext},
};
use crate::{
    StringSpan,
    names::{ColumnName, FullName, TableName},
    sem::{
        FunctionParam, OverloadVariant, ScalarConstExpr, SemScalarExpr, SourcePathWithSpan,
        TypeReference,
    },
};
use std::path::PathBuf;

/// Create independent cache state for a test-owned analysis base.
pub(crate) fn create_cache_context() -> CacheContext {
    CacheContext::new()
}

#[cfg(feature = "sqlite")]
#[expect(missing_docs)]
mod sqlite_syn_builder {
    #[cfg(feature = "sqlite")]
    use crate::SqliteDialect;
    use crate::sem::AnalysisError;
    #[cfg(feature = "sqlite")]
    use crate::syn::sqlite::Expr;
    #[cfg(feature = "sqlite")]
    use crate::syn::sqlite::Type;

    use turso_parser::parser::Parser as TursoParser;

    #[cfg(feature = "sqlite")]
    use crate::syn::sqlite::{Cmd, Name, QualifiedName, Stmt};

    #[derive(Default)]
    pub struct SqliteArrayTypeGate(usize);

    impl SqliteArrayTypeGate {
        pub const fn uses(&self) -> usize {
            self.0
        }
    }

    impl crate::engine_specific::TursoArrayTypeFeature for SqliteArrayTypeGate {
        fn array_type_used(&mut self, _: &Type) -> Result<(), AnalysisError> {
            self.0 += 1;
            Ok(())
        }
    }

    #[derive(Default)]
    pub struct SqliteSynBuilder;

    #[rstest::fixture]
    pub fn sqlite_syn() -> SqliteSynBuilder {
        SqliteSynBuilder
    }

    impl SqliteSynBuilder {
        pub fn parse_expression(&self, sql: &str) -> Expr {
            let mut parser = TursoParser::new(sql.as_bytes());
            let Some(Ok(turso_parser::ast::Cmd::Stmt(turso_parser::ast::Stmt::Select(select)))) =
                parser.next()
            else {
                panic!("expected SELECT: {sql}");
            };
            let turso_parser::ast::OneSelect::Select { mut columns, .. } = select.body.select
            else {
                panic!("expected expression SELECT");
            };
            let turso_parser::ast::ResultColumn::Expr(expr, _) = columns.remove(0) else {
                panic!("expected expression column");
            };
            Expr::from(*expr)
        }

        pub fn parse_command(&self, sql: &str) -> Cmd {
            SqliteDialect::parse(sql)
                .expect("valid SQLite command")
                .expect("command exists")
        }

        pub fn parse_as_stmt(&self, sql: &str) -> Stmt {
            self.parse_command(sql)
                .into_plain_statement()
                .expect("plain statement")
        }

        pub fn name(&self, value: &str) -> Name {
            #[cfg(feature = "sqlite")]
            use crate::syn::sqlite::NameBuilder;
            NameBuilder::default()
                .value(String::from(value))
                .build()
                .unwrap()
        }

        pub fn qualified_name(&self, name: &str) -> QualifiedName {
            #[cfg(feature = "sqlite")]
            use crate::syn::sqlite::QualifiedNameBuilder;

            QualifiedNameBuilder::default()
                .name(self.name(name))
                .build()
                .unwrap()
        }
    }
}

/// setup default function overload for test
pub fn setup_function_overload() -> (TypeReference, OverloadVariant) {
    let int4 = TypeReference::from_master_type_name("int4");
    let output_param =
        FunctionParam::new_output_param(&Some(String::from("val")), &Some(int4.clone()));
    let overload = OverloadVariant::new(&None, &vec![output_param], false, false);
    (int4, overload)
}

/// default SourcePathWithSpan
#[rstest::fixture]
pub fn source_span() -> SourcePathWithSpan {
    let source_one = PathBuf::from("source_one.sql");
    SourcePathWithSpan::new(&source_one, &StringSpan::new(0, 0))
}

/// semantic value builder for unit tests
pub struct SemValueBuilder {}

/// new semantic value builder
#[rstest::fixture]
pub fn sem_builder() -> SemValueBuilder {
    SemValueBuilder {}
}

impl SemValueBuilder {
    /// create TableName value
    pub fn table_name(&self, local_name: &str) -> TableName {
        TableName::from(FullName::from_local_name(local_name))
    }

    /// create column_name value
    pub fn column_name(&self, column_name: &str) -> ColumnName {
        ColumnName::from(column_name)
    }

    /// create constraint int expression
    pub fn const_int_expr(&self, value: i32) -> SemScalarExpr {
        SemScalarExpr::Const(ScalarConstExpr::Integer(value))
    }
}

/// Run a regression on an explicit default-sized stack, independent of Cargo's
/// `RUST_MIN_STACK`. Panics retain their original payload on the test thread.
pub fn on_small_stack<T: Send>(operation: impl FnOnce() -> T + Send) -> T {
    on_stack(2 * 1024 * 1024, operation)
}

fn on_stack<T: Send>(stack_size: usize, operation: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("small-stack-analysis".to_owned())
            .stack_size(stack_size)
            .spawn_scoped(scope, operation)
            .expect("small-stack worker");
        match worker.join() {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    })
}

/// Isolate an abort-prone regression, then run its body on an explicit stack.
pub fn small_stack_regression(stack_size: usize, operation: impl FnOnce() + Send) {
    let thread = std::thread::current();
    let name = thread.name().expect("named test thread");
    if std::env::var("SQL_FUN_SMALL_STACK_TEST").as_deref() == Ok(name) {
        on_stack(stack_size, operation);
    } else {
        let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", name, "--nocapture"])
            .env("SQL_FUN_SMALL_STACK_TEST", name)
            .env("RUST_LOG", "error")
            .output()
            .expect("isolated small-stack regression");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "small-stack regression {name} failed: {}\n{}",
            stdout,
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

/// Create a lightweight schema layer with the explicitly selected PostgreSQL test environment.
/// The supplied base contributes catalog data only.
pub(crate) fn schema_layer(path: &std::path::Path, base: &FrozenBaseContext) -> SchemaFileContext {
    use crate::context::BaseParseContext;
    let environment = TestParseContext::default();
    SchemaFileContext::new(path, base, environment.get_args(), environment.metadata())
}

#[rstest::fixture]
pub(crate) fn context_args() -> ::sql_fun_core::SqlFunArgs {
    <::sql_fun_core::SqlFunArgs as ::clap::Parser>::try_parse_from(vec![
        "sqlfun",
        "--metadata-file",
        "sql_fun.metadata.toml",
        "--sql-fun-home",
        env!("SQL_FUN_HOME"),
    ])
    .unwrap()
}

use crate::syn::Constraint;

pub(crate) fn constr_not_null() -> Constraint {
    SynBuilder::new().constraint_not_null()
}

pub(crate) fn constr_null() -> Constraint {
    SynBuilder::new().constraint_null()
}

pub(crate) fn constr_default() -> Constraint {
    SynBuilder::new().constraint_default()
}

pub(crate) fn constr_identity() -> Constraint {
    SynBuilder::new().constraint_identity("d")
}

pub(crate) fn constr_generated() -> Constraint {
    SynBuilder::new().constraint_generated("s")
}

pub(crate) fn constr_check() -> Constraint {
    SynBuilder::new().constraint_check()
}

pub(crate) fn constr_primary() -> Constraint {
    SynBuilder::new().constraint_primary_key()
}

pub(crate) fn constr_unique() -> Constraint {
    SynBuilder::new().constraint_unique()
}

pub(crate) fn constr_exclusion() -> Constraint {
    SynBuilder::new().constraint_exclusion()
}

pub(crate) fn constr_foreign() -> Constraint {
    SynBuilder::new().constraint_foreign()
}

pub(crate) fn constr_attr_deferrable() -> Constraint {
    SynBuilder::new().constraint_attr_deferrable()
}

pub(crate) fn constr_attr_not_deferrable() -> Constraint {
    SynBuilder::new().constraint_attr_not_deferrable()
}

pub(crate) fn constr_attr_deferred() -> Constraint {
    SynBuilder::new().constraint_attr_deferred()
}

pub(crate) fn constr_attr_immediate() -> Constraint {
    SynBuilder::new().constraint_attr_immediate()
}
