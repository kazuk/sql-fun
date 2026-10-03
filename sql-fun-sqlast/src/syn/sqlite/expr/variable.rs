use super::Expr;
use ::turso_parser::ast::Variable as VariableIn;
use std::num::NonZero;

/// variable expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct Variable {
    index: NonZero<u32>,
    name: Option<String>,
    col_type: Option<String>,
    #[getter(copy)]
    numbered: bool,
}

impl From<VariableIn> for Variable {
    fn from(value: VariableIn) -> Self {
        let VariableIn {
            index,
            name,
            col_type,
            numbered,
        } = value;
        Self {
            index,
            name: name.map(String::from),
            col_type: col_type.map(String::from),
            numbered,
        }
    }
}

#[cfg(test)]
mod test_variable_conversion {
    use super::{Variable, VariableIn};
    use crate::test_helpers::SqliteSynBuilder;
    use std::num::NonZeroU32;

    #[test]
    fn preserves_complete_parameter_names_and_numbered_markers() {
        for (spelling, name, numbered, index) in [
            ("$ns::name", Some("$ns::name"), false, 1),
            ("$name(element)", Some("$name(element)"), false, 1),
            ("$ns::name(element)", Some("$ns::name(element)"), false, 1),
            (":name", Some(":name"), false, 1),
            ("@name", Some("@name"), false, 1),
            ("?", None, false, 1),
            ("?1", None, true, 1),
            ("?12", None, true, 12),
        ] {
            let expr = SqliteSynBuilder.parse_expression(&format!("SELECT {spelling}"));
            let variable = expr.as_variable().unwrap();
            assert_eq!(variable.name().as_deref(), name);
            assert_eq!(variable.index().get(), index);
            assert_eq!(variable.numbered(), numbered);
            assert_eq!(variable.col_type(), &None);
        }
    }

    #[test]
    fn preserves_parser_supplied_column_type() {
        let variable = Variable::from(VariableIn::indexed_typed(
            NonZeroU32::new(2).unwrap(),
            "INTEGER",
        ));
        assert_eq!(variable.index().get(), 2);
        assert_eq!(variable.col_type().as_deref(), Some("INTEGER"));
        assert!(variable.name().is_none());
        assert!(!variable.numbered());
    }
}

impl Expr {
    /// create Variable variant
    pub fn variable<V>(var: V) -> Self
    where
        V: Into<Variable>,
    {
        Self::Variable(var.into())
    }
}
