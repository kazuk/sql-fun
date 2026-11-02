#![cfg(test)]
//! test helper types

mod postgres_ast;
pub(crate) use self::postgres_ast::{
    assert_postgres_structure, postgres_expression, postgres_node,
};
pub(crate) mod analysis_cache;
pub(crate) mod column_rename;

pub(crate) mod between;
pub(crate) mod collate;
pub(crate) mod common_types;
pub(crate) mod composite_domains;
pub(crate) mod conflict_masters;
pub(crate) mod declaration_collation;
pub(crate) mod domain_casts;
pub(crate) mod dynamic_types;
pub(crate) mod foreign_keys;
pub(crate) mod membership;
pub(crate) mod ownership;
pub(crate) mod parameter_calls;
pub(crate) mod pattern;
pub(crate) mod rows_from;
pub(crate) mod sampling;
pub(crate) mod set_operations;
mod syn_builder;
mod test_parse_context;

use std::path::PathBuf;

use crate::{
    StringSpan,
    sem::{
        ColumnName, FullName, FunctionParam, OverloadVariant, ScalarConstExpr, SemScalarExpr,
        SourcePathWithSpan, TableName, TypeReference,
    },
};

pub use self::{
    syn_builder::SynBuilder,
    test_parse_context::{TestParseContext, test_context},
};

/// Create independent cache state for a test-owned analysis base.
pub(crate) fn create_cache_context() -> crate::CacheContext {
    crate::CacheContext::new()
}

#[cfg(feature = "sqlite")]
#[expect(missing_docs)]
mod sqlite_syn_builder {
    use turso_parser::parser::Parser as TursoParser;

    use crate::syn::sqlite::{Cmd, Name, QualifiedName, Stmt};

    #[derive(Default)]
    pub struct SqliteArrayTypeGate(usize);

    impl SqliteArrayTypeGate {
        pub const fn uses(&self) -> usize {
            self.0
        }
    }

    impl crate::engine_specific::TursoArrayTypeFeature for SqliteArrayTypeGate {
        fn array_type_used(
            &mut self,
            _: &crate::syn::sqlite::Type,
        ) -> Result<(), crate::sem::AnalysisError> {
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
        pub fn parse_expression(&self, sql: &str) -> crate::syn::sqlite::Expr {
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
            crate::syn::sqlite::Expr::from(*expr)
        }

        pub fn parse_command(&self, sql: &str) -> Cmd {
            crate::SqliteDialect::parse(sql)
                .expect("valid SQLite command")
                .expect("command exists")
        }

        pub fn parse_as_stmt(&self, sql: &str) -> Stmt {
            self.parse_command(sql)
                .into_plain_statement()
                .expect("plain statement")
        }

        pub fn name(&self, value: &str) -> Name {
            use crate::syn::sqlite::NameBuilder;
            NameBuilder::default()
                .value(String::from(value))
                .build()
                .unwrap()
        }

        pub fn qualified_name(&self, name: &str) -> QualifiedName {
            use crate::syn::sqlite::QualifiedNameBuilder;

            QualifiedNameBuilder::default()
                .name(self.name(name))
                .build()
                .unwrap()
        }
    }
}

#[cfg(feature = "sqlite")]
pub use self::sqlite_syn_builder::{SqliteArrayTypeGate, SqliteSynBuilder, sqlite_syn};

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
pub(crate) fn schema_layer(
    path: &std::path::Path,
    base: &crate::context::FrozenBaseContext,
) -> crate::context::SchemaFileContext {
    use crate::context::BaseParseContext;
    let environment = TestParseContext::default();
    crate::context::SchemaFileContext::new(
        path,
        base,
        environment.get_args(),
        environment.metadata(),
    )
}
