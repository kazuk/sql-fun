use crate::sem::{
    AliasSpec, ColumnName, DataSource, JoinKind, Nullable, ParseContext, SemScalarExpr, WithClause,
    data_source::AliasName,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JoinUsingClause {
    using_columns: Vec<ColumnName>,
    alias: Option<AliasSpec>,
}

impl JoinUsingClause {
    pub(super) fn new(using_columns: &[ColumnName], alias: &Option<AliasSpec>) -> Self {
        Self {
            using_columns: using_columns.to_owned(),
            alias: alias.clone(),
        }
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JoinExprDataSource {
    kind: JoinKind,
    left: DataSource,
    right: DataSource,
    qualifications: Option<SemScalarExpr>,
    alias: Option<AliasSpec>,
    using_clause: Option<JoinUsingClause>,
    natural: bool,
}

impl JoinExprDataSource {
    pub(super) fn new_data_source(
        kind: &JoinKind,
        left: &DataSource,
        right: &DataSource,
        qualifications: &Option<SemScalarExpr>,
        alias: &Option<AliasSpec>,
        using_clause: &Option<JoinUsingClause>,
        natural: bool,
    ) -> DataSource {
        DataSource::JoinExpr(Box::new(Self {
            kind: kind.clone(),
            left: left.clone(),
            right: right.clone(),
            qualifications: qualifications.clone(),
            alias: alias.clone(),
            using_clause: using_clause.clone(),
            natural,
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
