#![cfg(test)]
//! test helper types

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

#[cfg(feature = "sqlite")]
#[expect(missing_docs)]
mod sqlite_syn_builder {
    use turso_parser::parser::Parser as TursoParser;

    use crate::syn::sqlite::{Name, QualifiedName, Stmt};

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

        pub fn parse_as_stmt(&self, sql: &str) -> Stmt {
            let mut parser = TursoParser::new(sql.as_bytes());
            let Some(cmd) = parser.next().transpose().unwrap() else {
                panic!("expect command")
            };
            let turso_parser::ast::Cmd::Stmt(stmt) = cmd else {
                panic!("expect command is statement")
            };
            Stmt::from(stmt)
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
pub use self::sqlite_syn_builder::{SqliteSynBuilder, sqlite_syn};

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
