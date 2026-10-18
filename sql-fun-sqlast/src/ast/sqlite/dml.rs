use crate::ast::{
    Alias, Assignment, Conflict, ConflictAction, Delete, Expr, FromItem, FromOpt, IndexExpression,
    Insert, Name, Query, Relation, Target, TryFromList, TryFromOpt, Update, With,
};
use crate::sem::{AnalysisError, ConflictClause};
use crate::syn::sqlite as syn;

impl TryFrom<&syn::InsertStatement> for Insert {
    type Error = AnalysisError;

    fn try_from(value: &syn::InsertStatement) -> Result<Self, Self::Error> {
        let source = InsertSource::try_from(value.body())?;
        Ok(Self {
            with: With::try_from_opt(value.with().as_ref())?.unwrap_or_default(),
            relation: Relation::sqlite_target(value.table_name(), None),
            columns: Assignment::insert_columns(value.columns()),
            source: source.query,
            overriding: None,
            conflict: source.conflict,
            or_conflict: ConflictClause::from_opt(value.or_conflict().as_ref()),
            returning: Target::try_from_list(value.returning())?,
        })
    }
}

#[derive(Default)]
struct InsertSource {
    query: Option<Query>,
    conflict: Option<Conflict>,
}

impl TryFrom<&syn::InsertBody> for InsertSource {
    type Error = AnalysisError;

    fn try_from(value: &syn::InsertBody) -> Result<Self, Self::Error> {
        match value {
            syn::InsertBody::Select(value) => Self::try_from(value.as_ref()),
            syn::InsertBody::DefaultValues => Ok(Self::default()),
        }
    }
}

impl TryFrom<&syn::InsertBodySelect> for InsertSource {
    type Error = AnalysisError;

    fn try_from(value: &syn::InsertBodySelect) -> Result<Self, Self::Error> {
        Ok(Self {
            query: Some(Query::try_from(value.select())?),
            conflict: Conflict::try_from_opt(value.upsert().as_deref())?,
        })
    }
}

impl TryFrom<&syn::Update> for Update {
    type Error = AnalysisError;

    fn try_from(value: &syn::Update) -> Result<Self, Self::Error> {
        Ok(Self {
            with: With::try_from_opt(value.with().as_ref())?.unwrap_or_default(),
            relation: Relation::sqlite_target(value.table_name(), value.indexed().as_ref()),
            assignments: Assignment::sqlite_sets(value.sets())?,
            from: FromItem::try_from_clause_opt(value.from_clause().as_ref())?,
            predicate: Expr::try_from_opt(value.where_clause().as_deref())?,
            or_conflict: ConflictClause::from_opt(value.or_conflict().as_ref()),
            returning: Target::try_from_list(value.returning())?,
        })
    }
}

#[cfg(test)]
mod test_update {
    use crate::ast::Update;
    use crate::test_helpers::SqliteSynBuilder;

    #[test]
    fn preserves_target_and_update_clauses() {
        let source = SqliteSynBuilder.parse_as_stmt(
            "WITH source AS (SELECT 2 AS v) UPDATE OR FAIL main.items AS target \
             INDEXED BY item_idx SET value = source.v, label = 'changed' \
             FROM source WHERE target.id = source.v RETURNING target.id AS changed_id",
        );
        let update = Update::try_from(source.as_update().expect("UPDATE")).expect("common UPDATE");
        assert_eq!(update.with.entries.len(), 1);
        assert_eq!(update.relation.name.0, ["main", "items"]);
        assert_eq!(update.relation.alias.as_ref().unwrap().name, "target");
        assert_eq!(update.relation.indexed.as_deref(), Some("item_idx"));
        assert!(!update.relation.not_indexed);
        assert!(update.or_conflict.is_some());
        assert_eq!(update.assignments.len(), 2);
        assert_eq!(update.assignments[0].name, "value");
        assert_eq!(update.assignments[1].name, "label");
        assert_eq!(update.from.len(), 1);
        assert!(update.predicate.is_some());
        assert_eq!(update.returning[0].alias.as_deref(), Some("changed_id"));
    }

    #[test]
    fn preserves_not_indexed_and_absent_clauses() {
        let source = SqliteSynBuilder.parse_as_stmt("UPDATE items NOT INDEXED SET value = 1");
        let update = Update::try_from(source.as_update().expect("UPDATE")).expect("common UPDATE");
        assert!(update.relation.not_indexed);
        assert!(update.relation.indexed.is_none());
        assert!(update.with.entries.is_empty());
        assert!(update.or_conflict.is_none());
        assert!(update.from.is_empty());
        assert!(update.predicate.is_none());
        assert!(update.returning.is_empty());
    }
}

impl TryFrom<&syn::DeleteStatement> for Delete {
    type Error = AnalysisError;

    fn try_from(value: &syn::DeleteStatement) -> Result<Self, Self::Error> {
        Ok(Self {
            with: With::try_from_opt(value.with().as_ref())?.unwrap_or_default(),
            relation: Relation::sqlite_target(value.table_name(), value.indexed().as_ref()),
            using: Vec::new(),
            predicate: Expr::try_from_opt(value.where_clause().as_deref())?,
            returning: Target::try_from_list(value.returning())?,
        })
    }
}

#[cfg(test)]
mod test_delete {
    use crate::ast::Delete;
    use crate::test_helpers::SqliteSynBuilder;

    #[test]
    fn preserves_target_and_delete_clauses() {
        let source = SqliteSynBuilder.parse_as_stmt(
            "WITH source AS (SELECT 2) DELETE FROM main.items AS target \
             NOT INDEXED WHERE id IN (SELECT * FROM source) RETURNING id AS removed",
        );
        let delete = Delete::try_from(source.as_delete().expect("DELETE")).expect("common DELETE");
        assert_eq!(delete.with.entries.len(), 1);
        assert_eq!(delete.relation.name.0, ["main", "items"]);
        assert_eq!(delete.relation.alias.as_ref().unwrap().name, "target");
        assert!(delete.relation.not_indexed);
        assert!(delete.predicate.is_some());
        assert_eq!(delete.returning[0].alias.as_deref(), Some("removed"));
    }
}

impl Relation {
    pub(in crate::ast) fn sqlite_target(
        name: &syn::QualifiedName,
        indexed: Option<&syn::Indexed>,
    ) -> Self {
        let (indexed, not_indexed) = match indexed {
            Some(syn::Indexed::IndexedBy(name)) => (Some(name.value().to_owned()), false),
            Some(syn::Indexed::NotIndexed) => (None, true),
            None => (None, false),
        };
        Self {
            name: Name::from(name),
            alias: Alias::from_opt(name.alias().as_ref()),
            inherit: false,
            persistence: String::new(),
            indexed,
            not_indexed,
        }
    }
}

impl From<&syn::Name> for Alias {
    fn from(value: &syn::Name) -> Self {
        Self {
            name: value.value().to_owned(),
            columns: Vec::new(),
        }
    }
}

impl Assignment {
    fn insert_columns(columns: &[syn::Name]) -> Vec<Self> {
        let mut assignments = Vec::new();
        for column in columns {
            assignments.push(Self {
                name: column.value().to_owned(),
                indirection: Vec::new(),
                value: None,
            });
        }
        assignments
    }

    fn sqlite_sets(sets: &[syn::Set]) -> Result<Vec<Self>, AnalysisError> {
        let mut assignments = Vec::new();
        for set in sets {
            let source = Expr::try_from(set.expr())?;
            let count =
                i32::try_from(set.col_names().len()).map_err(AnalysisError::generic_error)?;
            let mut position = 0;
            for column in set.col_names() {
                position += 1;
                let value = if count == 1 {
                    source.clone()
                } else {
                    Expr::multi_assignment(&source, position, count)
                };
                assignments.push(Self {
                    name: column.value().to_owned(),
                    indirection: Vec::new(),
                    value: Some(value),
                });
            }
        }
        Ok(assignments)
    }
}

impl TryFrom<&syn::Upsert> for Conflict {
    type Error = AnalysisError;

    fn try_from(value: &syn::Upsert) -> Result<Self, Self::Error> {
        crate::sem::with_stack(|| Self::sqlite_upsert(value))
    }
}

impl Conflict {
    fn sqlite_upsert(value: &syn::Upsert) -> Result<Self, AnalysisError> {
        let update = ConflictUpdate::try_from(value.do_clause())?;
        let (inference, inference_predicate) = if let Some(index) = value.index() {
            (
                IndexExpression::try_from_list(index.targets())?,
                Expr::try_from_opt(index.where_clause().as_deref())?,
            )
        } else {
            (Vec::new(), None)
        };
        Ok(Self {
            action: Some(update.action),
            inference,
            inference_predicate,
            constraint: None,
            assignments: update.assignments,
            predicate: update.predicate,
            next: Self::try_from_opt(value.next().as_deref())?.map(Box::new),
        })
    }
}

struct ConflictUpdate {
    action: ConflictAction,
    assignments: Vec<Assignment>,
    predicate: Option<Expr>,
}

impl ConflictAction {
    const fn nothing() -> Self {
        Self::Nothing
    }

    const fn update() -> Self {
        Self::Update
    }
}

impl ConflictUpdate {
    const fn nothing() -> Self {
        Self {
            action: ConflictAction::nothing(),
            assignments: Vec::new(),
            predicate: None,
        }
    }
}

impl TryFrom<&syn::UpsertDo> for ConflictUpdate {
    type Error = AnalysisError;

    fn try_from(value: &syn::UpsertDo) -> Result<Self, Self::Error> {
        match value {
            syn::UpsertDo::Set(value) => Self::try_from(value),
            syn::UpsertDo::Nothing => Ok(Self::nothing()),
        }
    }
}

impl TryFrom<&syn::UpsertDoSet> for ConflictUpdate {
    type Error = AnalysisError;

    fn try_from(value: &syn::UpsertDoSet) -> Result<Self, Self::Error> {
        Ok(Self {
            action: ConflictAction::update(),
            assignments: Assignment::sqlite_sets(value.sets())?,
            predicate: Expr::try_from_opt(value.where_clause().as_deref())?,
        })
    }
}

impl TryFrom<&syn::SortedColumn> for IndexExpression {
    type Error = AnalysisError;

    fn try_from(value: &syn::SortedColumn) -> Result<Self, Self::Error> {
        Ok(Self {
            name: None,
            alias: None,
            expression: Some(Expr::try_from(value.expr())?),
            collation: Name::default(),
            operator_class: Name::default(),
            options: Vec::new(),
            descending: value
                .order()
                .map(|order| matches!(order, syn::SortOrder::Desc)),
            using_order: false,
            nulls_first: value
                .nulls()
                .as_ref()
                .map(|order| matches!(order, syn::NullsOrder::First)),
        })
    }
}

impl TryFrom<&syn::TriggerCmd> for crate::ast::StatementNode {
    type Error = AnalysisError;

    fn try_from(value: &syn::TriggerCmd) -> Result<Self, Self::Error> {
        match value {
            syn::TriggerCmd::Update(value) => Ok(Self::update(Update::try_from(value)?)),
            syn::TriggerCmd::Insert(value) => Ok(Self::insert(Insert::try_from(value)?)),
            syn::TriggerCmd::Delete(value) => Ok(Self::delete(Delete::try_from(value)?)),
            syn::TriggerCmd::Select(value) => Ok(Self::query(Query::try_from(value)?)),
        }
    }
}

impl Relation {
    fn sqlite_trigger_target(name: &syn::Name) -> Self {
        Self {
            name: Name::from_parts(&[name.value().to_owned()]),
            alias: None,
            inherit: false,
            persistence: String::new(),
            indexed: None,
            not_indexed: false,
        }
    }
}

impl TryFrom<&syn::TriggerCmdUpdate> for Update {
    type Error = AnalysisError;

    fn try_from(value: &syn::TriggerCmdUpdate) -> Result<Self, Self::Error> {
        Ok(Self {
            with: With::default(),
            relation: Relation::sqlite_trigger_target(value.table_name()),
            assignments: Assignment::sqlite_sets(value.sets())?,
            from: FromItem::try_from_clause_opt(value.from().as_ref())?,
            predicate: Expr::try_from_opt(value.where_clause().as_deref())?,
            or_conflict: ConflictClause::from_opt(value.or_conflict().as_ref()),
            returning: Vec::new(),
        })
    }
}

impl TryFrom<&syn::TriggerCmdInsert> for Insert {
    type Error = AnalysisError;

    fn try_from(value: &syn::TriggerCmdInsert) -> Result<Self, Self::Error> {
        Ok(Self {
            with: With::default(),
            relation: Relation::sqlite_trigger_target(value.table_name()),
            columns: Assignment::insert_columns(value.col_names()),
            source: Some(Query::try_from(value.select())?),
            overriding: None,
            conflict: Conflict::try_from_opt(value.upsert().as_deref())?,
            or_conflict: ConflictClause::from_opt(value.or_conflict().as_ref()),
            returning: Target::try_from_list(value.returning())?,
        })
    }
}

#[cfg(test)]
mod test_trigger_insert {
    use crate::ast::Insert;
    use crate::syn::sqlite::{InsertBody, Stmt, TriggerCmdInsert};
    use crate::test_helpers::SqliteSynBuilder;

    #[test]
    fn retains_adapter_returning_field_even_though_sql_parser_rejects_clause() {
        let Stmt::Insert(insert) =
            SqliteSynBuilder.parse_as_stmt("INSERT INTO audit(a) SELECT 1 RETURNING a AS recorded")
        else {
            panic!("INSERT fixture")
        };
        let InsertBody::Select(source) = insert.body() else {
            panic!("SELECT source")
        };
        let command = TriggerCmdInsert::new(
            None,
            SqliteSynBuilder.name("audit"),
            insert.columns().clone(),
            source.select().clone(),
            None,
            insert.returning().clone(),
        );
        let common = Insert::try_from(&command).expect("trigger command conversion");
        assert_eq!(common.returning[0].alias.as_deref(), Some("recorded"));
        assert_eq!(common.columns[0].name, "a");
    }
}

impl TryFrom<&syn::TriggerCmdDelete> for Delete {
    type Error = AnalysisError;

    fn try_from(value: &syn::TriggerCmdDelete) -> Result<Self, Self::Error> {
        Ok(Self {
            with: With::default(),
            relation: Relation::sqlite_trigger_target(value.table_name()),
            using: Vec::new(),
            predicate: Expr::try_from_opt(value.where_clause().as_deref())?,
            returning: Vec::new(),
        })
    }
}
