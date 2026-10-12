use crate::{
    ast::{
        Column, ColumnConstraint, Expr, ForeignReference, Name, Query, Table, TableConstraint,
        TypeName,
    },
    sem::{AnalysisError, ConflictClause},
    syn::sqlite as syn,
};

impl TryFrom<&syn::AlterTable> for crate::ast::AlterRelation {
    type Error = AnalysisError;
    fn try_from(value: &syn::AlterTable) -> Result<Self, Self::Error> {
        Ok(Self {
            only: false,
            name: Name::from(value.name()),
            kind: crate::ast::AlterRelationKind::table(),
            commands: vec![crate::ast::AlterCommand::try_from(value.body())?],
            if_exists: false,
        })
    }
}

impl TryFrom<&syn::AlterTableBody> for crate::ast::AlterCommand {
    type Error = AnalysisError;
    fn try_from(value: &syn::AlterTableBody) -> Result<Self, Self::Error> {
        Ok(match value {
            syn::AlterTableBody::RenameTo(name) => {
                Self::Rename(Name::from_parts(&[name.value().to_owned()]))
            }
            syn::AlterTableBody::AddColumn(column) => Self::AddColumn(Column::try_from(column)?),
            syn::AlterTableBody::AlterColumn(name, column) => {
                Self::AlterColumn(name.value().to_owned(), Column::try_from(column)?)
            }
            syn::AlterTableBody::RenameColumn(from, to) => {
                Self::RenameColumn(from.value().to_owned(), to.value().to_owned())
            }
            syn::AlterTableBody::DropColumn(name) => Self::DropColumn(name.value().to_owned()),
        })
    }
}

impl TryFrom<&syn::CreateTableStatement> for Table {
    type Error = AnalysisError;
    fn try_from(value: &syn::CreateTableStatement) -> Result<Self, Self::Error> {
        let mut result = Self::try_from(value.body())?;
        result.name = Name::from(value.table_name());
        result.temporary = *value.temporary();
        result.mode = crate::sem::CreateTableMode::from_if_not_exists(*value.if_not_exists());
        Ok(result)
    }
}

impl TryFrom<&syn::ColumnsAndConstraint> for Table {
    type Error = AnalysisError;
    fn try_from(value: &syn::ColumnsAndConstraint) -> Result<Self, Self::Error> {
        let mut options = Vec::new();
        if value.options().without_rowid_text().is_some() {
            options.push(crate::ast::SyntaxOption {
                namespace: None,
                name: "without_rowid".to_owned(),
                value: None,
            });
        }
        if value.options().strict_text().is_some() {
            options.push(crate::ast::SyntaxOption {
                namespace: None,
                name: "strict".to_owned(),
                value: None,
            });
        }
        Ok(Self {
            extras: crate::ast::TableExtras::default(),
            name: Name::from_parts(&[]),
            columns: value
                .columns()
                .iter()
                .map(Column::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            constraints: value
                .constraints()
                .iter()
                .map(TableConstraint::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            query: None,
            temporary: false,
            mode: crate::sem::CreateTableMode::Default,
            options,
        })
    }
}

impl TryFrom<&syn::CreateTableBody> for Table {
    type Error = AnalysisError;
    fn try_from(value: &syn::CreateTableBody) -> Result<Self, Self::Error> {
        match value {
            syn::CreateTableBody::ColumnsAndConstraint(body) => Self::try_from(body),
            syn::CreateTableBody::AsSelect(query) => Ok(Self {
                extras: crate::ast::TableExtras::default(),
                name: Name::from_parts(&[]),
                columns: Vec::new(),
                constraints: Vec::new(),
                query: Some(Query::try_from(query)?),
                temporary: false,
                mode: crate::sem::CreateTableMode::Default,
                options: Vec::new(),
            }),
        }
    }
}

impl TryFrom<&syn::ColumnDefinition> for Column {
    type Error = AnalysisError;
    fn try_from(value: &syn::ColumnDefinition) -> Result<Self, Self::Error> {
        let constraints = value
            .constraints()
            .iter()
            .map(TableConstraint::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            collation: Name::default(),
            compression: None,
            storage: None,
            name: value.column_name().value().to_owned(),
            type_name: value.column_type().map(TypeName::try_from).transpose()?,
            constraints,
            not_null: None,
        })
    }
}

impl TryFrom<&syn::NamedColumnConstraint> for TableConstraint {
    type Error = AnalysisError;
    fn try_from(value: &syn::NamedColumnConstraint) -> Result<Self, Self::Error> {
        Ok(Self {
            extras: crate::ast::ConstraintExtras::default(),
            name: value.name().map(|name| name.value().to_owned()),
            keys: Vec::new(),
            constraint: ColumnConstraint::try_from(value.constraint())?,
            order: crate::ast::Order::sqlite_column_primary_key(value.constraint()),
            auto_increment: matches!(value.constraint(), syn::ColumnConstraint::PrimaryKey(primary) if primary.auto_increment()),
        })
    }
}

impl crate::ast::Order {
    fn sqlite_column_primary_key(constraint: &syn::ColumnConstraint) -> Vec<Self> {
        let syn::ColumnConstraint::PrimaryKey(primary) = constraint else {
            return Vec::new();
        };
        let Some(direction) = primary.order() else {
            return Vec::new();
        };
        vec![Self {
            expression: Expr::null(),
            descending: Some(matches!(direction, syn::SortOrder::Desc)),
            nulls_first: None,
            using: None,
        }]
    }
}

impl TryFrom<&syn::ColumnConstraint> for ColumnConstraint {
    type Error = AnalysisError;
    fn try_from(value: &syn::ColumnConstraint) -> Result<Self, Self::Error> {
        Ok(match value {
            syn::ColumnConstraint::PrimaryKey(value) => {
                Self::PrimaryKey(value.conflict_clause().map(ConflictClause::from), None)
            }
            syn::ColumnConstraint::NotNull(value) => Self::Nullability(
                !value.nullable(),
                value.conflict_clause().map(ConflictClause::from),
                None,
            ),
            syn::ColumnConstraint::Unique(value) => {
                Self::Unique(value.conflict_clause().map(ConflictClause::from))
            }
            syn::ColumnConstraint::Check(value) => Self::Check(Expr::try_from(value.expr())?),
            syn::ColumnConstraint::Default(value) => Self::Default(Expr::try_from(value.expr())?),
            syn::ColumnConstraint::Collate(value) => {
                Self::Collate(value.collation_name().value().to_owned())
            }
            syn::ColumnConstraint::ForeignKey(value) => {
                Self::ForeignKey(ForeignReference::try_from(value)?)
            }
            syn::ColumnConstraint::Generated(value) => Self::Generated(
                Expr::try_from(value.expr().as_ref())?,
                value
                    .typ()
                    .as_ref()
                    .map(crate::sem::GeneratedColumnMode::from),
            ),
        })
    }
}

impl TryFrom<&syn::ForeignKeyColumnConstraint> for ForeignReference {
    type Error = AnalysisError;
    fn try_from(value: &syn::ForeignKeyColumnConstraint) -> Result<Self, Self::Error> {
        Self::from_sqlite(value.clause(), value.defer_clause().as_ref())
    }
}

impl ForeignReference {
    fn from_sqlite(
        value: &syn::ForeignKeyClause,
        defer: Option<&syn::DeferSubclause>,
    ) -> Result<Self, AnalysisError> {
        use crate::sem::{Deferrability, ForeignKeyAction};
        let mut result = Self {
            table: Name::from_parts(&[value.table_name().value().to_owned()]),
            columns: value
                .columns()
                .iter()
                .map(|column| column.column_name().value().to_owned())
                .collect(),
            match_type: None,
            delete_action: None,
            update_action: None,
            deferrability: defer.map(|defer| {
                Deferrability::new(
                    *defer.deferrable(),
                    defer.init_deferred().unwrap_or_default().init_deferred(),
                )
            }),
            validity: None,
            ignored_matches: Vec::new(),
        };
        for arg in value.args() {
            match arg {
                syn::RefArg::OnDelete(action) => {
                    result.delete_action = Some(ForeignKeyAction::from_ref_act(action))
                }
                syn::RefArg::OnUpdate(action) => {
                    result.update_action = Some(ForeignKeyAction::from_ref_act(action))
                }
                syn::RefArg::Match(name) => result.ignored_matches.push(name.value().to_owned()),
                syn::RefArg::OnInsert(_) => {
                    return AnalysisError::raise_unexpected_input("OnInsert in FK args");
                }
            }
        }
        Ok(result)
    }
}

impl TryFrom<&syn::NamedTableConstraint> for TableConstraint {
    type Error = AnalysisError;
    fn try_from(value: &syn::NamedTableConstraint) -> Result<Self, Self::Error> {
        let order = match value.constraint() {
            syn::TableConstraint::PrimaryKey(primary) => primary
                .columns()
                .iter()
                .map(crate::ast::Order::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            syn::TableConstraint::Unique(unique) => unique
                .columns()
                .iter()
                .map(crate::ast::Order::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            _ => Vec::new(),
        };
        let (keys, constraint) = match value.constraint() {
            syn::TableConstraint::PrimaryKey(value) => (
                Vec::new(),
                ColumnConstraint::PrimaryKey(
                    value.conflict_clause().as_ref().map(ConflictClause::from),
                    None,
                ),
            ),
            syn::TableConstraint::Unique(value) => (
                Vec::new(),
                ColumnConstraint::Unique(
                    value.conflict_clause().as_ref().map(ConflictClause::from),
                ),
            ),
            syn::TableConstraint::Check(value) => (
                Vec::new(),
                ColumnConstraint::Check(Expr::try_from(value.expr().as_ref())?),
            ),
            syn::TableConstraint::ForeignKey(value) => (
                value
                    .columns()
                    .iter()
                    .map(|column| column.column_name().value().to_owned())
                    .collect(),
                ColumnConstraint::ForeignKey(ForeignReference::from_sqlite(
                    value.clause(),
                    value.defer_clause().as_ref(),
                )?),
            ),
        };
        Ok(Self {
            extras: crate::ast::ConstraintExtras::default(),
            name: value.name().as_ref().map(|name| name.value().to_owned()),
            keys,
            constraint,
            order,
            auto_increment: matches!(value.constraint(), syn::TableConstraint::PrimaryKey(primary) if *primary.auto_increment()),
        })
    }
}
