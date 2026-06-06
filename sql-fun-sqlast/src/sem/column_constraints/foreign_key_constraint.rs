use crate::{
    sem::{
        AnalysisError, AnalysisProblem, ColumnConstraint, ColumnConstraintKind,
        ColumnConstraintTrait, ColumnName, ParseContext, TableName, TypeReference, Validity,
    },
    syn::{ConstrType, ListOpt, Opt},
};

/// foreign key match type
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub enum ForeignKeyMatchType {
    /// simple
    Simple,
    /// full
    Full,
    /// partial
    ///
    /// # Note
    ///
    /// partial match type not implemented in PostgreSQL 18, AST value defined but not used.
    ///
    Partial,
}

impl ForeignKeyMatchType {
    /// convert from [`crate::syn::Constraint`]
    pub fn try_from_postgres_constraint(
        syn: &crate::syn::Constraint,
    ) -> Result<Option<Self>, AnalysisError> {
        let value = syn.get_fk_matchtype();
        match value.as_str() {
            "" => Ok(None),
            "s" => Ok(Some(Self::Simple)),
            "f" => Ok(Some(Self::Full)),
            "p" => Ok(Some(Self::Partial)),
            _ => AnalysisError::raise_unexpected_input("fk_matchtype"),
        }
    }
}

/// foreign key update action
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub enum ForeignKeyAction {
    /// no-action
    NoAction,
    /// restrict
    Restrict,
    /// set null value
    SetNull,
    /// set default
    SetDefault,
    /// cascade
    Cascade,
}

impl ForeignKeyAction {
    fn try_from_value(value: &str, source: &str) -> Result<Self, AnalysisError> {
        match value {
            "a" => Ok(Self::NoAction),
            "r" => Ok(Self::Restrict),
            "c" => Ok(Self::Cascade),
            "n" => Ok(Self::SetNull),
            "d" => Ok(Self::SetDefault),
            _ => AnalysisError::raise_unexpected_input(source),
        }
    }

    /// get delete action from [`crate::syn::Constraint`]
    pub fn try_from_delete_postgres_constraint(
        syn: &crate::syn::Constraint,
    ) -> Result<Option<Self>, AnalysisError> {
        let del_action = syn.get_fk_del_action();
        if del_action.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Self::try_from_value(&del_action, "fk_del_action")?))
        }
    }

    /// get update action from [`crate::syn::Constraint`]
    pub fn try_from_update_postgres_constraint(
        syn: &crate::syn::Constraint,
    ) -> Result<Option<Self>, AnalysisError> {
        let upd_action = syn.get_fk_upd_action();
        if upd_action.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Self::try_from_value(&upd_action, "fk_upd_action")?))
        }
    }
}

/// deferrable / not deferrable, initially deferred
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct Deferrability {
    deferrable: bool,
    init_deferred: bool,
}

impl Deferrability {
    fn from_postgres_constraint(syn: &crate::syn::Constraint) -> Self {
        let deferrable = syn.get_deferrable();
        let init_deferred = syn.get_initdeferred();
        Self {
            deferrable,
            init_deferred,
        }
    }
}

/// foreign key constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct ForeignKeyColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    primary_key_table: TableName,
    primary_key_column: ColumnName,
    primary_key_column_type: Option<TypeReference>,
    match_type: Option<ForeignKeyMatchType>,
    delete_action: Option<ForeignKeyAction>,
    update_action: Option<ForeignKeyAction>,
    deferrablity: Option<Deferrability>,
    validity: Option<Validity>,
}

impl ForeignKeyColumnConstraint {
    /// create instance
    #[expect(clippy::too_many_arguments)]
    pub fn new(
        table_name: &TableName,
        column_name: &ColumnName,
        primary_key_table: &TableName,
        primary_key_column: &ColumnName,
        primary_key_column_type: Option<&TypeReference>,
        match_type: Option<&ForeignKeyMatchType>,
        delete_action: Option<ForeignKeyAction>,
        update_action: Option<ForeignKeyAction>,
        deferrablity: Option<Deferrability>,
        validity: Option<Validity>,
    ) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            primary_key_table: primary_key_table.clone(),
            primary_key_column: primary_key_column.clone(),
            primary_key_column_type: primary_key_column_type.cloned(),
            match_type: match_type.cloned(),
            delete_action,
            update_action,
            deferrablity,
            validity,
        }
    }

    /// get primary key table name
    pub fn primary_key_table(&self) -> &TableName {
        &self.primary_key_table
    }

    /// get primary key column name
    pub fn primary_key_column(&self) -> &ColumnName {
        &self.primary_key_column
    }

    /// get primary key column type
    pub fn primary_key_column_type(&self) -> Option<&TypeReference> {
        self.primary_key_column_type.as_ref()
    }

    /// get match type
    pub fn match_type(&self) -> Option<&ForeignKeyMatchType> {
        self.match_type.as_ref()
    }
}

impl ColumnConstraintTrait for ForeignKeyColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::ForeignKey
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ForeignKeyColumnConstraint {
    /// analyze [`crate::syn::Constraint`]
    pub fn analyze_constraint<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &crate::syn::Constraint,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(contype) = syn.get_contype().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.contype")?;
            AnalysisError::never()
        };
        if !matches!(contype, ConstrType::ConstrForeign) {
            AnalysisError::raise_unexpected_input("contype not foreign")?;
            AnalysisError::never()
        }

        let fk_match_type = ForeignKeyMatchType::try_from_postgres_constraint(syn)?;

        let Some(pktable) = syn.get_pktable().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.pktable")?;
            AnalysisError::never()
        };

        let primary_key_table = TableName::try_from(pktable)?;
        let Some(pkattr) = syn.get_pk_attrs().as_inner() else {
            AnalysisError::raise_unexpected_none("constraint.pk_attrs")?;
            AnalysisError::never()
        };
        if pkattr.len() != 1 {
            AnalysisError::raise_unexpected_input("constraint.pk_attrs.len!=1")?;
            AnalysisError::never()
        }
        let primary_key_column = ColumnName::try_from(&pkattr[0])?;

        let primary_key_column_type = if let Some(pk_table) = context.get_table(&primary_key_table)
        {
            if let Some(pk_col_def) = pk_table.get_column_def(&primary_key_column) {
                pk_col_def.get_type()
            } else {
                context.report_problem(AnalysisProblem::column_not_found_in_table(
                    &primary_key_table,
                    &primary_key_column,
                ))?;
                None
            }
        } else {
            context.report_problem(AnalysisProblem::table_not_found(&primary_key_table))?;
            None
        };

        let del_action = ForeignKeyAction::try_from_delete_postgres_constraint(syn)?;
        let upd_action = ForeignKeyAction::try_from_update_postgres_constraint(syn)?;
        let deferrability = Deferrability::from_postgres_constraint(syn);
        let validity = Validity::from_constraint(syn);

        Ok((
            Self::new(
                table_name,
                column_name,
                &primary_key_table,
                &primary_key_column,
                primary_key_column_type,
                fk_match_type.as_ref(),
                del_action,
                upd_action,
                Some(deferrability),
                Some(validity),
            ),
            context,
        ))
    }
}

impl ColumnConstraint {
    /// create ForeignKey variant
    pub fn foreign_key(value: ForeignKeyColumnConstraint) -> Self {
        Self::ForeignKey(value)
    }

    /// get ForeignKey variant
    pub fn as_foreign_key(&self) -> Option<&ForeignKeyColumnConstraint> {
        if let Self::ForeignKey(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
