//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{
        Column as AstColumn, ColumnConstraint as AstColumnConstraint, Dialect as AstDialect,
        TypeName as AstTypeName,
    },
    context::ParseContext,
    names::{FullName, LocalName, TableName},
    sem::{AnalysisError, AnalysisProblem, CreateSequence},
};
use ::sql_fun_core::analysis_problem::SerialProblem;

#[derive(Default)]
pub(crate) struct SerialSequences(Vec<CreateSequence>);

impl SerialSequences {
    pub(crate) fn into_sequences(self) -> Vec<CreateSequence> {
        self.0
    }

    pub(in crate::sem::create_table) fn stage(
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
    use crate::sem::create_table::serial::serial_name;

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

impl SerialSequences {
    pub(crate) fn integer_for_ast_column(
        ast: &AstColumn,
        context: &impl ParseContext,
    ) -> Option<&'static str> {
        if context.dialect() != AstDialect::postgres() {
            return None;
        }
        Self::integer_for_ast_type(ast.type_name().as_ref()?)
    }

    pub(crate) fn validate_ast_column(
        ast: &AstColumn,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let syntax = ast.type_name().as_ref().expect("serial declaration type");
        let mut problems = Vec::new();
        if !syntax.array_bounds().is_empty() {
            problems.push(SerialProblem::array());
        }
        if !syntax.modifiers().is_empty() {
            problems.push(SerialProblem::modifier());
        }
        for constraint in ast.constraints() {
            if let Some(problem) = Self::conflict_from_ast_constraint(constraint.constraint()) {
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

impl SerialSequences {
    pub(crate) fn integer_for_ast_type(ast: &AstTypeName) -> Option<&'static str> {
        if ast.percent_type() || ast.set_of() {
            return None;
        }
        let [name] = ast.name().parts() else {
            return None;
        };
        match name.as_str() {
            "smallserial" | "serial2" => Some("int2"),
            "serial" | "serial4" => Some("int4"),
            "bigserial" | "serial8" => Some("int8"),
            _ => None,
        }
    }

    pub(crate) const fn conflict_from_ast_constraint(
        ast: &AstColumnConstraint,
    ) -> Option<SerialProblem> {
        match ast {
            AstColumnConstraint::Nullability(false, _, _) => Some(SerialProblem::nullable()),
            AstColumnConstraint::Default(_) => Some(SerialProblem::default_conflict()),
            AstColumnConstraint::Identity(_) => Some(SerialProblem::identity()),
            AstColumnConstraint::Generated(_, _) => Some(SerialProblem::generated()),
            _ => None,
        }
    }
}
