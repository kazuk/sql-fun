use crate::{
    ast::{Call, CallKind, Column, ColumnConstraint, Expr, Literal, Name, TypeName},
    names::FullName,
};

impl Column {
    /// Expand the selected SERIAL spelling into ordinary integer declaration syntax.
    /// The semantic consumer selects the integer type and stages catalog effects.
    pub(crate) fn serial_expansion(&self, integer: &str) -> Self {
        let mut expanded = self.clone();
        let syntax = expanded
            .type_name
            .as_mut()
            .expect("serial declaration type");
        syntax.name = Name::from_parts(&["pg_catalog".to_owned(), integer.to_owned()]);
        syntax.array_bounds.clear();
        expanded
    }
}

impl ColumnConstraint {
    pub(crate) const fn serial_not_null() -> Self {
        Self::Nullability(true, None, None)
    }
    pub(crate) fn serial_default(sequence: &FullName) -> Self {
        Self::Default(Expr::serial_next_value(sequence))
    }
}

impl Expr {
    pub(crate) fn serial_next_value(sequence: &FullName) -> Self {
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
    pub(crate) fn sequence_reference(sequence: &FullName) -> Self {
        Self::String(sequence.to_string())
    }
}

impl Call {
    pub(crate) fn serial_next_value(argument: &Expr) -> Self {
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
