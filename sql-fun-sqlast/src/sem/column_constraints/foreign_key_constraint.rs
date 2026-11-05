use crate::{
    ast::{Deferrability, ForeignKeyAction, ForeignKeyMatchType, Validity},
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnRename,
        ForeignKeyTarget, ParseContext, TypeReference,
    },
    syn::{ConstrType, Constraint, ListOpt, Opt},
};
#[cfg(feature = "sqlite")]
use crate::{
    sem::AnalysisProblem,
    syn::sqlite::ForeignKeyColumnConstraint as SqliteForeignKeyColumnConstraint,
};

/// foreign key constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct ForeignKeyColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    primary_key_table: ForeignKeyTarget,
    primary_key_column: ColumnName,
    primary_key_column_type: Option<TypeReference>,
    match_type: Option<ForeignKeyMatchType>,
    delete_action: Option<ForeignKeyAction>,
    update_action: Option<ForeignKeyAction>,
    deferrablity: Option<Deferrability>,
    validity: Option<Validity>,
}

impl ForeignKeyColumnConstraint {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
        if self
            .primary_key_table
            .relation()
            .is_some_and(|table| rename.matches_table(table))
        {
            rename.rename_name(&mut self.primary_key_column);
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
        self.primary_key_table.rename_relation(old, new);
    }

    /// Create a constraint with a canonical target, or `None` for unresolved lookup.
    ///
    /// The caller supplies a completed lookup result, never an unqualified SQL name.
    #[expect(clippy::too_many_arguments)]
    pub fn new(
        table_name: &TableName,
        column_name: &ColumnName,
        primary_key_table: Option<&TableName>,
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
            primary_key_table: ForeignKeyTarget::new(primary_key_table),
            primary_key_column: primary_key_column.clone(),
            primary_key_column_type: primary_key_column_type.cloned(),
            match_type: match_type.cloned(),
            delete_action,
            update_action,
            deferrablity,
            validity,
        }
    }
}

#[cfg(test)]
mod test_foreign_key_serialization {
    use crate::{
        names::{ColumnName, FullName},
        sem::{ForeignKeyColumnConstraint, PgBuiltInType},
        test_helpers::foreign_keys as fk,
    };

    #[test]
    fn inline_type_and_current_serialization_preserve_completed_outcomes() -> testresult::TestResult
    {
        let context = fk::bind(
            fk::parents(),
            "child",
            "parent(id) ON DELETE CASCADE NOT DEFERRABLE",
            false,
        );
        let constraint = fk::inline(&context, &FullName::with_schema("a", "child"));
        assert_eq!(
            constraint.primary_key_column_type(),
            Some(&PgBuiltInType::int4())
        );
        assert_eq!(constraint.primary_key_column(), &ColumnName::from("id"));
        let json = ::serde_json::to_value(&constraint)?;
        assert_eq!(json["delete_action"], "Cascade");
        assert_eq!(json["deferrablity"]["deferrable"], false);
        let restored: ForeignKeyColumnConstraint = ::serde_json::from_value(json)?;
        assert_eq!(restored, constraint);
        let context = fk::bind(context, "unresolved", "missing(id)", false);
        let unresolved = fk::inline(&context, &FullName::with_schema("a", "unresolved"));
        assert!(unresolved.primary_key_table().is_none());
        let json = ::serde_json::to_value(&unresolved)?;
        assert!(json["primary_key_table"].is_null());
        let restored: ForeignKeyColumnConstraint = ::serde_json::from_value(json)?;
        assert_eq!(restored, unresolved);
        Ok(())
    }
}

impl ForeignKeyColumnConstraint {
    /// Canonical referenced relation, or `None` after failed target lookup.
    ///
    /// Later search-path changes do not alter this retained binding.
    pub const fn primary_key_table(&self) -> Option<&TableName> {
        self.primary_key_table.relation()
    }

    /// get primary key column name
    pub const fn primary_key_column(&self) -> &ColumnName {
        &self.primary_key_column
    }

    /// get primary key column type
    pub const fn primary_key_column_type(&self) -> Option<&TypeReference> {
        self.primary_key_column_type.as_ref()
    }

    /// get match type
    pub const fn match_type(&self) -> Option<&ForeignKeyMatchType> {
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
        syn: &Constraint,
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

        let (target, primary_key_column_type) = ForeignKeyTarget::resolve_column(
            &mut context,
            &primary_key_table,
            &primary_key_column,
        )?;

        let del_action = ForeignKeyAction::try_from_delete_postgres_constraint(syn)?;
        let upd_action = ForeignKeyAction::try_from_update_postgres_constraint(syn)?;
        let deferrability = Deferrability::from_postgres_constraint(syn);
        let validity = Validity::from_constraint(syn);

        Ok((
            Self::new(
                table_name,
                column_name,
                target.relation(),
                &primary_key_column,
                primary_key_column_type.as_ref(),
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

#[cfg(test)]
mod test_analyze_constraint {
    use crate::{
        names::ColumnName,
        sem::{ForeignKeyColumnConstraint, PgBuiltInType},
        test_helpers::foreign_keys as fk,
    };

    #[rstest::rstest]
    #[case::resolved("parent", true)]
    #[case::missing("absent", false)]
    fn direct_analysis_retains_completed_lookup(
        #[case] target: &str,
        #[case] resolved: bool,
    ) -> testresult::TestResult {
        let context = fk::parents();
        let syntax = fk::postgres_constraint(&format!(
            "ALTER TABLE a.child ADD FOREIGN KEY(id) REFERENCES {target}(id) ON UPDATE CASCADE"
        ));
        let (constraint, mut context) = ForeignKeyColumnConstraint::analyze_constraint(
            context,
            &fk::name("a", "child"),
            &ColumnName::from("id"),
            &syntax,
        )?;
        let expected = if resolved {
            Some(fk::name("a", "parent"))
        } else {
            None
        };
        assert_eq!(constraint.primary_key_table(), expected.as_ref());
        if resolved {
            assert_eq!(
                constraint.primary_key_column_type(),
                Some(&PgBuiltInType::int4())
            );
            assert!(context.problems().is_empty(), "{:?}", context.problems());
        } else {
            assert_eq!(context.problems()[0].code(), "table_not_found");
        }
        fk::reversed_path(&mut context);
        assert_eq!(constraint.primary_key_table(), expected.as_ref());
        Ok(())
    }
}

#[cfg(feature = "sqlite")]
impl ForeignKeyColumnConstraint {
    /// analyze [`crate::syn::sqlite::ForeignKeyColumnConstraint`]
    pub fn analyze_constraint_sqlite<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &SqliteForeignKeyColumnConstraint,
    ) -> Result<(ColumnConstraint, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let clause = syn.clause();
        let mut del_action = None;
        let mut upd_action = None;
        for arg in clause.args() {
            #[cfg(feature = "sqlite")]
            use crate::syn::sqlite::RefArg;

            match arg {
                RefArg::OnDelete(del) => del_action = Some(ForeignKeyAction::from_ref_act(del)),
                RefArg::OnUpdate(upd) => upd_action = Some(ForeignKeyAction::from_ref_act(upd)),
                RefArg::OnInsert(_) => {
                    AnalysisError::raise_unexpected_input("RefArg::OnInsert in FK args")?;
                }
                RefArg::Match(name) => context.report_analysis_problem(
                    AnalysisProblem::sqlite_ignores_match_option_in_fk_constraint(
                        table_name,
                        column_name,
                        name.value(),
                    ),
                )?,
            }
        }

        let pk_columns = clause.columns();
        if pk_columns.len() != 1 {
            AnalysisError::raise_unexpected_input("clause.columns.len!=1")?;
            AnalysisError::never()
        }
        let pk_column = ColumnName::from(pk_columns[0].column_name());
        let pk_table = TableName::try_from(clause.table_name())?;

        let (target, pk_column_type) =
            ForeignKeyTarget::resolve_column(&mut context, &pk_table, &pk_column)?;
        let deferrability = syn
            .defer_clause()
            .as_ref()
            .map(Deferrability::from_defer_subclause);

        let fk = Self::new(
            table_name,
            column_name,
            target.relation(),
            &pk_column,
            pk_column_type.as_ref(),
            None,
            del_action,
            upd_action,
            deferrability,
            None,
        );

        Ok((ColumnConstraint::foreign_key(fk), context))
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_analyze_constraint_sqlite {
    #[cfg(feature = "sqlite")]
    use crate::syn::sqlite::ColumnConstraint;
    use crate::{
        names::{ColumnName, FullName},
        sem::ForeignKeyColumnConstraint,
        test_helpers::{SqliteSynBuilder, foreign_keys as fk},
    };

    #[test]
    fn direct_sqlite_analysis_retains_the_canonical_target() -> testresult::TestResult {
        let context = fk::apply_sqlite(fk::sqlite_context(), "CREATE TABLE Parent(id PRIMARY KEY)");
        let syntax = SqliteSynBuilder.parse_as_stmt("CREATE TABLE child(id REFERENCES PARENT(id))");
        let table = syntax.as_create_table().unwrap();
        let body = table.body().as_columns_and_constraint().unwrap();
        let ColumnConstraint::ForeignKey(constraint) =
            body.columns()[0].constraints()[0].constraint()
        else {
            panic!("foreign-key syntax");
        };
        let (result, context) = ForeignKeyColumnConstraint::analyze_constraint_sqlite(
            context,
            &fk::name("main", "child"),
            &ColumnName::from("id"),
            constraint,
        )?;
        assert_eq!(
            result
                .as_foreign_key()
                .unwrap()
                .primary_key_table()
                .unwrap()
                .full_name(),
            &FullName::with_schema("main", "parent")
        );
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        Ok(())
    }
}

impl ColumnConstraint {
    /// create ForeignKey variant
    pub const fn foreign_key(value: ForeignKeyColumnConstraint) -> Self {
        Self::ForeignKey(value)
    }

    /// get ForeignKey variant
    pub const fn as_foreign_key(&self) -> Option<&ForeignKeyColumnConstraint> {
        if let Self::ForeignKey(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
