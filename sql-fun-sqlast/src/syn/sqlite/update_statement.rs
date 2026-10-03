use ::turso_parser::ast::Update as UpdateIn;

use crate::syn::sqlite::{
    Expr, FromClause, Indexed, Limit, Name, QualifiedName, ResolveType, ResultColumn, SortedColumn,
    With,
};

/// update statement
#[derive(derive_getters::Getters, Debug)]
pub struct Update {
    with: Option<With>,
    or_conflict: Option<ResolveType>,
    table_name: QualifiedName,
    indexed: Option<Indexed>,
    sets: Vec<Set>,
    from_clause: Option<FromClause>,
    where_clause: Option<Box<Expr>>,
    returning: Vec<ResultColumn>,
    order_by: Vec<SortedColumn>,
    limit: Option<Limit>,
}

impl From<UpdateIn> for Update {
    fn from(value: UpdateIn) -> Self {
        let UpdateIn {
            with,
            or_conflict,
            tbl_name,
            indexed,
            sets,
            from,
            where_clause,
            returning,
            order_by,
            limit,
        } = value;
        Self {
            with: with.map(With::from),
            or_conflict: or_conflict.map(ResolveType::from),
            table_name: QualifiedName::from(tbl_name),
            indexed: indexed.map(Indexed::from),
            sets: sets.into_iter().map(Set::from).collect(),
            from_clause: from.map(FromClause::from),
            where_clause: where_clause.map(Expr::from).map(Box::new),
            returning: returning.into_iter().map(ResultColumn::from).collect(),
            order_by: order_by.into_iter().map(SortedColumn::from).collect(),
            limit: limit.map(Limit::from),
        }
    }
}

#[cfg(test)]
mod test_update_conversion {
    use crate::test_helpers::SqliteSynBuilder;

    #[test]
    fn preserves_update_clauses_after_input_is_dropped() {
        let statement = {
            let sql = String::from(
                "WITH source AS (SELECT 2 AS v) UPDATE OR FAIL main.items AS target \
                 INDEXED BY item_idx SET value = source.v, label = 'changed' \
                 FROM source JOIN other ON source.v = other.v \
                 WHERE target.id = other.id RETURNING target.id AS changed_id, value \
                 ORDER BY value DESC NULLS LAST LIMIT 3 OFFSET 1",
            );
            SqliteSynBuilder.parse_as_stmt(&sql)
        };
        let update = statement.as_update().expect("UPDATE");
        assert_eq!(update.with().as_ref().unwrap().ctes().len(), 1);
        assert!(update.or_conflict().is_some());
        assert_eq!(update.table_name().db_name().unwrap().value(), "main");
        assert_eq!(update.table_name().name().value(), "items");
        assert_eq!(
            update.table_name().alias().as_ref().unwrap().value(),
            "target"
        );
        assert!(update.indexed().is_some());
        assert_eq!(update.sets().len(), 2);
        assert_eq!(update.sets()[0].col_names()[0].value(), "value");
        assert_eq!(update.from_clause().as_ref().unwrap().joins().len(), 1);
        assert!(update.where_clause().is_some());
        assert_eq!(update.returning().len(), 2);
        assert_eq!(update.order_by().len(), 1);
        assert!(update.order_by()[0].order().is_some());
        assert!(update.order_by()[0].nulls().is_some());
        assert!(update.limit().as_ref().unwrap().offset().is_some());
    }

    #[test]
    fn retains_absent_update_clauses() {
        let statement = SqliteSynBuilder.parse_as_stmt("UPDATE items SET value = 1");
        let update = statement.as_update().expect("UPDATE");
        assert!(update.with().is_none());
        assert!(update.or_conflict().is_none());
        assert!(update.indexed().is_none());
        assert!(update.from_clause().is_none());
        assert!(update.where_clause().is_none());
        assert!(update.returning().is_empty());
        assert!(update.order_by().is_empty());
        assert!(update.limit().is_none());
    }
}

use turso_parser::ast::Set as SetIn;

/// set clause of update statement
#[derive(derive_getters::Getters, Debug)]
pub struct Set {
    col_names: Vec<Name>,
    expr: Box<Expr>,
}

impl From<SetIn> for Set {
    fn from(value: SetIn) -> Self {
        Self {
            col_names: value.col_names.into_iter().map(Name::from).collect(),
            expr: Box::new(Expr::from(*value.expr)),
        }
    }
}
