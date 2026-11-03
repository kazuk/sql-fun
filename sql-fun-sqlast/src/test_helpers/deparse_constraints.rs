use crate::ast::{ColumnConstraint, TableConstraint};

pub(crate) fn constraint(
    name: Option<&str>,
    keys: &[&str],
    constraint: ColumnConstraint,
) -> TableConstraint {
    TableConstraint::test_new(name, keys, constraint)
}
