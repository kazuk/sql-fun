//! PostgreSQL serial declaration expansion; ordinary type lookup stays unchanged.

use crate::{
    ast::{Call, CallKind, Column, ColumnConstraint, Dialect, Expr, Literal, Name, TypeName},
    context::ParseContext,
    names::{ColumnName, FullName, LocalName, TableName},
    sem::{AnalysisError, AnalysisProblem, ColumnDefinition, CreateSequence},
};
use ::sql_fun_core::analysis_problem::SerialProblem;

/// Sequences staged by one CREATE TABLE, before any catalog publication.
#[derive(Default)]
pub(super) struct SerialSequences(Vec<CreateSequence>);

impl SerialSequences {
    pub(super) fn into_sequences(self) -> Vec<CreateSequence> {
        self.0
    }

    fn stage(
        &mut self,
        context: &impl ParseContext,
        table: &TableName,
        column: &str,
        temporary: bool,
    ) -> FullName {
        let name = self.next_name(context, table, column);
        let mut sequence = CreateSequence::new(&name).with_temporary(temporary);
        sequence.set_owned_by(table);
        self.0.push(sequence);
        name
    }

    fn next_name(&self, context: &impl ParseContext, table: &TableName, column: &str) -> FullName {
        for suffix in 0_u64.. {
            let label = if suffix == 0 {
                "seq".to_owned()
            } else {
                format!("seq{suffix}")
            };
            let local = serial_name(table.full_name().name().as_str(), column, &label);
            let name = FullName::new(
                &table.full_name().catalog().cloned(),
                &table.full_name().schema().cloned(),
                &LocalName::from(local.as_str()),
            );
            if context.object_kind(&name).is_none()
                && &name != table.full_name()
                && !self.0.iter().any(|sequence| sequence.name() == &name)
            {
                return name;
            }
        }
        unreachable!("serial sequence name space exhausted")
    }
}

// PostgreSQL makeObjectName: balance byte lengths, then clip at UTF-8 boundaries.
fn serial_name(table: &str, column: &str, label: &str) -> String {
    let (mut left, mut right) = (table.len(), column.len());
    let available = 63 - label.len() - 2;
    while left + right > available {
        if left > right {
            left -= 1;
        } else {
            right -= 1;
        }
    }
    while !table.is_char_boundary(left) {
        left -= 1;
    }
    while !column.is_char_boundary(right) {
        right -= 1;
    }
    format!("{}_{}_{}", &table[..left], &column[..right], label)
}

#[cfg(test)]
mod serial_name_tests {
    use crate::ast::ddl::serial::serial_name;

    #[rstest::rstest]
    #[case::ordinary("users", "id", "seq", "users_id_seq")]
    #[case::collision("users", "id", "seq1", "users_id_seq1")]
    #[case::balanced(
        "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz",
        "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz",
        "seq",
        "abcdefghijklmnopqrstuvwxyzabc_abcdefghijklmnopqrstuvwxyzabc_seq"
    )]
    #[case::utf8(
        "ああああああああああああああああああああ",
        "いいいいいいいいいいいいいいいいいいいい",
        "seq",
        "あああああああああ_いいいいいいいいい_seq"
    )]
    fn follows_postgres_name_clipping(
        #[case] table: &str,
        #[case] column: &str,
        #[case] label: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(serial_name(table, column, label), expected);
    }
}

impl Column {
    pub(super) fn resolve_serial(
        &self,
        context: &mut impl ParseContext,
        table: &TableName,
        temporary: bool,
        sequences: &mut SerialSequences,
    ) -> Result<ColumnDefinition, AnalysisError> {
        let Some(integer) = self.serial_integer(context) else {
            return self.resolve(context, table);
        };
        let mut expanded = self.clone();
        let syntax = expanded
            .type_name
            .as_mut()
            .expect("serial declaration type");
        syntax.name = Name::from_parts(&["pg_catalog".to_owned(), integer.to_owned()]);
        syntax.array_bounds.clear();
        let valid = self.validate_serial(context)?;
        let mut column = expanded.resolve(context, table)?;
        if !valid {
            column.reject_declaration();
            return Ok(column);
        }
        let owner = TableName::from(context.creation_name(table.full_name(), temporary)?);
        let sequence_name = sequences.stage(context, &owner, &self.name, temporary);
        let name = ColumnName::from(self.name.as_str());
        let not_null = ColumnConstraint::serial_not_null();
        column.push_constraint(not_null.resolve(context, table, &name)?);
        column.set_not_null(Some(true));
        let default = ColumnConstraint::serial_default(&sequence_name);
        column.push_constraint(default.resolve(context, table, &name)?);
        Ok(column)
    }

    fn serial_integer(&self, context: &impl ParseContext) -> Option<&'static str> {
        if context.dialect() != Dialect::postgres() {
            return None;
        }
        self.type_name.as_ref()?.serial_integer()
    }

    fn validate_serial(&self, context: &mut impl ParseContext) -> Result<bool, AnalysisError> {
        let syntax = self.type_name.as_ref().expect("serial declaration type");
        let mut problems = Vec::new();
        if !syntax.array_bounds.is_empty() {
            problems.push(SerialProblem::array());
        }
        if !syntax.modifiers.is_empty() {
            problems.push(SerialProblem::modifier());
        }
        for constraint in &self.constraints {
            if let Some(problem) = constraint.constraint.serial_conflict() {
                problems.push(problem);
            }
        }
        let valid = problems.is_empty();
        for problem in problems {
            context.report_problem(AnalysisProblem::serial(problem))?;
        }
        Ok(valid)
    }
}

#[cfg(test)]
mod serial_catalog_tests {
    use crate::{
        context::{ParseContext, SequenceInfoRead, TableInfoRead},
        names::{ColumnName, FullName, TableName},
        sem::{PgBuiltInType, TypeReference},
        test_helpers::{ownership::apply_ownership_sql, serial},
    };

    #[rstest::rstest]
    #[case::smallserial(
        "smallserial",
        TypeReference::from_full_name(FullName::with_schema("pg_catalog", "int2"))
    )]
    #[case::serial2(
        "serial2",
        TypeReference::from_full_name(FullName::with_schema("pg_catalog", "int2"))
    )]
    #[case::serial("serial", PgBuiltInType::int4())]
    #[case::serial4("serial4", PgBuiltInType::int4())]
    #[case::bigserial("bigserial", PgBuiltInType::int8())]
    #[case::serial8("serial8", PgBuiltInType::int8())]
    #[case::quoted(r#""serial""#, PgBuiltInType::int4())]
    fn retains_serial_effects(
        #[case] spelling: &str,
        #[case] expected: TypeReference,
    ) -> testresult::TestResult {
        let pair = apply_ownership_sql(
            serial::context(),
            &format!("CREATE TABLE users(id {spelling})"),
        )?;
        let table = pair.ast().as_create_table().unwrap();
        assert!(table.declarations_can_apply());
        let column = table.find_column(&ColumnName::from("id"), false).unwrap();
        assert_eq!(column.get_type(), Some(&expected));
        assert_eq!(column.is_not_null(), Some(true));
        let default = column.serial_default_expression().unwrap();
        assert_eq!(
            default.function_dependency(),
            Some(&FullName::with_schema("pg_catalog", "nextval"))
        );
        assert!(table.primary_key_columns().is_empty());
        let context = pair.into_context();
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let sequence = context
            .get_sequence_impl(&FullName::with_schema("public", "users_id_seq"))
            .unwrap();
        assert_eq!(
            sequence.owned_by(),
            Some(&TableName::from(FullName::with_schema("public", "users")))
        );
        let identity = sequence.name().clone();
        let frozen = context.freeze();
        assert!(frozen.get_sequence_impl(&identity).is_some());
        Ok(())
    }

    #[rstest::rstest]
    #[case::nullable("serial NULL")]
    #[case::default("serial DEFAULT 1")]
    #[case::identity("serial GENERATED ALWAYS AS IDENTITY")]
    #[case::array("serial[]")]
    #[case::modifier("serial(3)")]
    fn rejected_serial_does_not_publish_objects(
        #[case] declaration: &str,
    ) -> testresult::TestResult {
        let pair = apply_ownership_sql(
            serial::context(),
            &format!("CREATE TABLE users(id {declaration})"),
        )?;
        assert!(
            !pair
                .ast()
                .as_create_table()
                .unwrap()
                .declarations_can_apply()
        );
        let context = pair.into_context();
        assert!(
            context
                .get_table_impl(&FullName::with_schema("public", "users"))
                .is_none()
        );
        assert!(
            context
                .get_sequence_impl(&FullName::with_schema("public", "users_id_seq"))
                .is_none()
        );
        assert!(
            context
                .problems()
                .iter()
                .any(|p| p.code() == "serial_invalid_declaration")
        );
        let context = apply_ownership_sql(context, "CREATE TABLE good(id integer)")?.into_context();
        assert!(
            context
                .get_table_impl(&FullName::with_schema("public", "good"))
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn skipped_creation_and_name_collisions_preserve_sequences() -> testresult::TestResult {
        let context = apply_ownership_sql(serial::context(), "CREATE TABLE users(id integer)")?
            .into_context();
        let context = apply_ownership_sql(context, "CREATE TABLE IF NOT EXISTS users(id serial)")?
            .into_context();
        assert!(
            context
                .get_sequence_impl(&FullName::with_schema("public", "users_id_seq"))
                .is_none()
        );
        let context = apply_ownership_sql(context, "CREATE SEQUENCE other_id_seq")?.into_context();
        let context = apply_ownership_sql(context, "CREATE TABLE other(id serial, second serial)")?
            .into_context();
        assert!(
            context
                .get_sequence_impl(&FullName::with_schema("public", "other_id_seq1"))
                .is_some()
        );
        assert!(
            context
                .get_sequence_impl(&FullName::with_schema("public", "other_second_seq"))
                .is_some()
        );
        Ok(())
    }
    #[test]
    fn index_conflict_leaves_no_serial_sequence() -> testresult::TestResult {
        let pair = apply_ownership_sql(
            serial::context(),
            "CREATE TABLE users(id serial CONSTRAINT users_id_seq PRIMARY KEY)",
        )?;
        let context = pair.into_context();
        assert!(
            context
                .get_table_impl(&FullName::with_schema("public", "users"))
                .is_none()
        );
        assert!(
            context
                .get_sequence_impl(&FullName::with_schema("public", "users_id_seq"))
                .is_none()
        );
        assert!(
            context
                .problems()
                .iter()
                .any(|p| p.code() == "conflicting_creation")
        );
        Ok(())
    }
}

impl TypeName {
    fn serial_integer(&self) -> Option<&'static str> {
        if self.percent_type || self.set_of {
            return None;
        }
        let [name] = self.name.0.as_slice() else {
            return None;
        };
        match name.as_str() {
            "smallserial" | "serial2" => Some("int2"),
            "serial" | "serial4" => Some("int4"),
            "bigserial" | "serial8" => Some("int8"),
            _ => None,
        }
    }
}

impl ColumnConstraint {
    const fn serial_conflict(&self) -> Option<SerialProblem> {
        match self {
            Self::Nullability(false, _, _) => Some(SerialProblem::nullable()),
            Self::Default(_) => Some(SerialProblem::default_conflict()),
            Self::Identity(_) => Some(SerialProblem::identity()),
            Self::Generated(_, _) => Some(SerialProblem::generated()),
            _ => None,
        }
    }
    const fn serial_not_null() -> Self {
        Self::Nullability(true, None, None)
    }
    fn serial_default(sequence: &FullName) -> Self {
        Self::Default(Expr::serial_next_value(sequence))
    }
}

impl Expr {
    fn serial_next_value(sequence: &FullName) -> Self {
        let literal = Self::Literal(Literal::sequence_reference(sequence));
        let target = TypeName::projection(
            &Name::from_parts(&["pg_catalog".to_owned(), "regclass".to_owned()]),
            false,
        );
        let argument = Self::projection_cast(&literal, &target);
        Self::projection_call(&Call::serial_next_value(&argument))
    }
}

impl Literal {
    fn sequence_reference(sequence: &FullName) -> Self {
        Self::String(sequence.to_string())
    }
}

impl Call {
    fn serial_next_value(argument: &Expr) -> Self {
        Self {
            sql_syntax: false,
            kind: CallKind::ordinary(),
            name: Name::from_parts(&["pg_catalog".to_owned(), "nextval".to_owned()]),
            arguments: vec![argument.clone()],
            argument_locations: vec![None],
            star: false,
            distinct: false,
            variadic: false,
            within_group: false,
            within_order: Vec::new(),
            order: Vec::new(),
            filter: None,
            over: None,
            location: None,
        }
    }
}
