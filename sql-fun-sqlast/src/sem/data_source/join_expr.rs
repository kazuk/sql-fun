use crate::sem::{
    AliasSpec, ColumnName, DataSource, JoinKind, Nullable, ParseContext, WithClause,
    data_source::AliasName,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JoinExprDataSource {
    kind: JoinKind,
    left: DataSource,
    right: DataSource,
}

impl JoinExprDataSource {
    pub(super) fn new_data_source(
        kind: &JoinKind,
        left: &DataSource,
        right: &DataSource,
    ) -> DataSource {
        DataSource::JoinExpr(Box::new(Self {
            kind: kind.clone(),
            left: left.clone(),
            right: right.clone(),
        }))
    }

    pub(super) fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        self.left.push_alias_names(names);
        self.right.push_alias_names(names);
    }

    pub(super) fn resolve_alias<'a>(
        &'a self,
        name: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        if let Some((left_source, left_nullable)) = self.left.resolve_alias(name) {
            Some((left_source, self.kind.left_nullable(left_nullable)))
        } else {
            let (right_source, right_nullable) = self.right.resolve_alias(name)?;
            Some((right_source, self.kind.right_nullable(right_nullable)))
        }
    }

    pub(super) fn get_aliases_for_column<TParseContext>(
        &self,
        column: &ColumnName,
        context: &TParseContext,
        with_clause: &WithClause,
        names: &mut Vec<AliasSpec>,
    ) where
        TParseContext: ParseContext,
    {
        let mut left_aliases = self
            .left
            .get_aliases_for_column(column, context, with_clause);
        let mut right_aliases = self
            .right
            .get_aliases_for_column(column, context, with_clause);
        names.append(&mut left_aliases);
        names.append(&mut right_aliases);
    }
}
