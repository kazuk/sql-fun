use super::{DeparseError, SqlEmit, SqlWriter};
use crate::ast::{
    Column, ColumnConstraint, ConstraintExtras, ExclusionElement, ForeignReference, SyntaxOption,
    SyntaxValue, TableConstraint,
};
use crate::sem::{
    ConflictClause, Deferrability, ForeignKeyAction, ForeignKeyMatchType, GeneratedColumnMode,
    IdentityColumnMode,
};

impl SqlEmit for Column {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.ident(&self.name)?;
        if let Some(type_name) = &self.type_name {
            writer.sql(" ");
            type_name.emit(writer)?;
        } else if writer.is_postgres() {
            writer.sql(" WITH OPTIONS");
        }
        if let Some(storage) = &self.storage {
            writer.postgres_only("column storage")?;
            let storage = match storage.to_ascii_uppercase().as_str() {
                "PLAIN" => "PLAIN",
                "EXTERNAL" => "EXTERNAL",
                "EXTENDED" => "EXTENDED",
                "MAIN" => "MAIN",
                "DEFAULT" => "DEFAULT",
                _ => return writer.invalid("column storage", "unknown PostgreSQL storage mode"),
            };
            writer.sql(" STORAGE ");
            writer.sql(storage);
        }
        if let Some(compression) = &self.compression {
            writer.postgres_only("column compression")?;
            writer.sql(" COMPRESSION ");
            writer.ident(compression)?;
        }
        if !self.collation.is_empty() {
            writer.sql(" COLLATE ");
            self.collation.emit(writer)?;
        }
        if self.not_null == Some(true) && !self.has_nullability_constraint() {
            writer.sql(" NOT NULL");
        }
        for constraint in &self.constraints {
            writer.sql(" ");
            constraint.emit_as_column(writer)?;
        }
        Ok(())
    }
}

impl Column {
    fn has_nullability_constraint(&self) -> bool {
        self.constraints
            .iter()
            .any(|constraint| constraint.constraint.is_nullability())
    }
}

impl ColumnConstraint {
    const fn is_nullability(&self) -> bool {
        matches!(self, Self::Nullability(..))
    }

    const fn is_primary_or_unique(&self) -> bool {
        matches!(self, Self::PrimaryKey(..) | Self::Unique(_))
    }

    const fn is_unique(&self) -> bool {
        matches!(self, Self::Unique(_))
    }

    const fn is_exclusion(&self) -> bool {
        matches!(self, Self::Exclusion)
    }

    const fn is_check(&self) -> bool {
        matches!(self, Self::Check(_))
    }

    const fn is_foreign_key(&self) -> bool {
        matches!(self, Self::ForeignKey(_))
    }

    const fn is_identity(&self) -> bool {
        matches!(self, Self::Identity(_))
    }

    const fn supports_deferrability(&self) -> bool {
        matches!(
            self,
            Self::PrimaryKey(..) | Self::Unique(_) | Self::ForeignKey(_) | Self::Exclusion
        )
    }
}

impl SqlEmit for TableConstraint {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.emit_with_context(writer, false)
    }
}

impl TableConstraint {
    pub(in crate::ast) fn emit_as_column(
        &self,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        self.emit_with_context(writer, true)
    }

    fn emit_with_context(
        &self,
        writer: &mut SqlWriter,
        column_context: bool,
    ) -> Result<(), DeparseError> {
        if let Some(name) = &self.name {
            writer.sql("CONSTRAINT ");
            writer.ident(name)?;
            writer.sql(" ");
        }
        if column_context {
            self.constraint.emit_column_with(writer, self)?;
        } else {
            self.constraint.emit_with(writer, self)?;
        }
        self.extras.emit_for(writer, &self.constraint)?;
        Ok(())
    }
}

impl ColumnConstraint {
    fn emit_with(
        &self,
        writer: &mut SqlWriter,
        owner: &TableConstraint,
    ) -> Result<(), DeparseError> {
        self.emit_with_context(writer, owner, false)
    }

    fn emit_column_with(
        &self,
        writer: &mut SqlWriter,
        owner: &TableConstraint,
    ) -> Result<(), DeparseError> {
        self.emit_with_context(writer, owner, true)
    }

    fn emit_with_context(
        &self,
        writer: &mut SqlWriter,
        owner: &TableConstraint,
        column_context: bool,
    ) -> Result<(), DeparseError> {
        match self {
            Self::Nullability(not_null, conflict, _validity) => {
                if !column_context {
                    return writer.invalid(
                        "nullability constraint",
                        "nullability is a column constraint",
                    );
                }
                writer.sql(if *not_null { "NOT NULL" } else { "NULL" });
                emit_conflict_clause(conflict.as_ref(), writer)?;
            }
            Self::Default(expression) => {
                if !column_context {
                    return writer.invalid("DEFAULT constraint", "DEFAULT is a column constraint");
                }
                writer.sql("DEFAULT (");
                expression.emit(writer)?;
                writer.sql(")");
            }
            Self::Check(expression) => {
                writer.sql("CHECK (");
                expression.emit(writer)?;
                writer.sql(")");
                if owner.extras.no_inherit {
                    writer.postgres_only("CHECK NO INHERIT")?;
                    writer.sql(" NO INHERIT");
                }
            }
            Self::PrimaryKey(conflict, deferrable) => {
                writer.sql("PRIMARY KEY");
                if column_context {
                    if !owner.keys.is_empty() {
                        return writer.invalid(
                            "column PRIMARY KEY",
                            "an inline primary key cannot have a key list",
                        );
                    }
                    emit_inline_primary_order(owner, writer)?;
                } else if let Some(index_name) = &owner.extras.index_name {
                    writer.postgres_only("PRIMARY KEY USING INDEX")?;
                    if !owner.keys.is_empty() || !owner.order.is_empty() {
                        return writer.invalid(
                            "PRIMARY KEY USING INDEX",
                            "an existing index target cannot have a key list",
                        );
                    }
                    writer.sql(" USING INDEX ");
                    writer.ident(index_name)?;
                } else if !owner.keys.is_empty() || !owner.order.is_empty() {
                    writer.sql(" (");
                    emit_keys(owner, writer)?;
                    writer.sql(")");
                } else {
                    return writer
                        .invalid("PRIMARY KEY", "table primary key requires at least one key");
                }
                emit_conflict_clause(conflict.as_ref(), writer)?;
                if !owner.extras.deferrable
                    && !owner.extras.initially_deferred
                    && let Some(deferrable) = deferrable
                {
                    writer.postgres_only("constraint deferrability")?;
                    writer.sql(" DEFERRABLE");
                    if deferrable.is_initially_deferred() {
                        writer.sql(" INITIALLY DEFERRED");
                    }
                }
                if owner.auto_increment {
                    if !column_context {
                        return writer.invalid(
                            "AUTOINCREMENT",
                            "SQLite AUTOINCREMENT is only valid on a column primary key",
                        );
                    }
                    if writer.is_postgres() {
                        return writer.invalid("AUTOINCREMENT", "SQLite-only column constraint");
                    }
                    writer.sql(" AUTOINCREMENT");
                }
            }
            Self::Unique(conflict) => {
                writer.sql("UNIQUE");
                if owner.extras.nulls_not_distinct {
                    writer.postgres_only("UNIQUE NULLS NOT DISTINCT")?;
                    writer.sql(" NULLS NOT DISTINCT");
                }
                if column_context {
                    if !owner.keys.is_empty() || !owner.order.is_empty() {
                        return writer.invalid("UNIQUE", "inline UNIQUE cannot have a key list");
                    }
                } else if let Some(index_name) = &owner.extras.index_name {
                    writer.postgres_only("UNIQUE USING INDEX")?;
                    if !owner.keys.is_empty() || !owner.order.is_empty() {
                        return writer.invalid(
                            "UNIQUE USING INDEX",
                            "an existing index target cannot have a key list",
                        );
                    }
                    writer.sql(" USING INDEX ");
                    writer.ident(index_name)?;
                } else if !owner.keys.is_empty() || !owner.order.is_empty() {
                    writer.sql(" (");
                    emit_keys(owner, writer)?;
                    writer.sql(")");
                } else {
                    return writer.invalid(
                        "UNIQUE",
                        "table UNIQUE constraint requires at least one key",
                    );
                }
                emit_conflict_clause(conflict.as_ref(), writer)?;
            }
            Self::Identity(mode) => {
                if !column_context {
                    return writer.invalid("identity column", "identity is a column constraint");
                }
                mode.emit(writer)?;
                emit_identity_options(&owner.extras.options, writer)?;
            }
            Self::Generated(expression, mode) => {
                if !column_context {
                    return writer.invalid("generated column", "generated is a column constraint");
                }
                if writer.is_postgres() && mode.is_none() {
                    return writer.invalid(
                        "generated column",
                        "PostgreSQL generated columns require a storage mode",
                    );
                }
                writer.sql("GENERATED ALWAYS AS (");
                expression.emit(writer)?;
                writer.sql(")");
                if let Some(mode) = mode {
                    writer.sql(" ");
                    mode.emit(writer)?;
                }
            }
            Self::ForeignKey(reference) => {
                reference.emit_with(writer, owner, column_context)?;
            }
            Self::Exclusion => {
                if column_context {
                    return writer.invalid("EXCLUDE constraint", "EXCLUDE is a table constraint");
                }
                writer.postgres_only("EXCLUDE constraint")?;
                if owner.extras.index_name.is_some() {
                    return writer.invalid(
                        "EXCLUDE constraint",
                        "exclusion constraints cannot use an existing index",
                    );
                }
                writer.sql("EXCLUDE");
                if let Some(method) = &owner.extras.access_method {
                    writer.sql(" USING ");
                    writer.ident(method)?;
                }
                if owner.extras.exclusions.is_empty() {
                    return writer.invalid(
                        "EXCLUDE constraint",
                        "exclusion constraint requires an element",
                    );
                }
                writer.sql(" (");
                writer.list(&owner.extras.exclusions, ", ")?;
                writer.sql(")");
            }
            Self::Deferrability(deferrable) => {
                writer.postgres_only("constraint deferrability")?;
                writer.sql(if *deferrable {
                    "DEFERRABLE"
                } else {
                    "NOT DEFERRABLE"
                });
            }
            Self::Deferred => {
                writer.postgres_only("constraint deferrability")?;
                writer.sql("INITIALLY DEFERRED");
            }
            Self::Immediate => {
                writer.postgres_only("constraint deferrability")?;
                writer.sql("INITIALLY IMMEDIATE");
            }
            Self::Collate(name) => {
                if !column_context {
                    return writer.invalid("COLLATE constraint", "COLLATE is a column constraint");
                }
                writer.sql("COLLATE ");
                writer.ident(name)?;
            }
        }
        Ok(())
    }
}

impl ForeignReference {
    fn emit_with(
        &self,
        writer: &mut SqlWriter,
        owner: &TableConstraint,
        column_context: bool,
    ) -> Result<(), DeparseError> {
        if column_context {
            if !owner.keys.is_empty() {
                return writer.invalid("REFERENCES", "inline foreign key cannot have a key list");
            }
        } else {
            if owner.keys.is_empty() {
                return writer
                    .invalid("FOREIGN KEY", "table foreign key requires at least one key");
            }
            writer.sql("FOREIGN KEY (");
            writer.names(&owner.keys, ", ")?;
            writer.sql(") ");
        }
        writer.sql("REFERENCES ");
        self.table.emit(writer)?;
        if !self.columns.is_empty() {
            writer.sql(" (");
            writer.names(&self.columns, ", ")?;
            writer.sql(")");
        }
        if let Some(match_type) = &self.match_type {
            writer.sql(" MATCH ");
            match_type.emit(writer)?;
        }
        for match_name in &self.ignored_matches {
            writer.sql(" MATCH ");
            writer.ident(match_name)?;
        }
        if let Some(action) = &self.update_action {
            writer.sql(" ON UPDATE ");
            action.emit(writer)?;
        }
        if let Some(action) = &self.delete_action {
            writer.sql(" ON DELETE ");
            action.emit(writer)?;
            if !owner.extras.delete_columns.is_empty() {
                writer.postgres_only("ON DELETE SET NULL column list")?;
                if !matches!(action, ForeignKeyAction::SetNull) {
                    return writer.invalid(
                        "ON DELETE column list",
                        "a column list is valid only with SET NULL",
                    );
                }
                writer.sql(" (");
                writer.names(&owner.extras.delete_columns, ", ")?;
                writer.sql(")");
            }
        } else if !owner.extras.delete_columns.is_empty() {
            return writer.invalid(
                "ON DELETE column list",
                "a column list requires an ON DELETE SET NULL action",
            );
        }
        if !(owner.extras.deferrable || owner.extras.initially_deferred)
            && let Some(deferrability) = &self.deferrability
        {
            writer.sql(" ");
            deferrability.emit(writer)?;
        }
        Ok(())
    }
}

impl SqlEmit for ForeignReference {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let owner = TableConstraint {
            extras: ConstraintExtras::default(),
            name: None,
            keys: Vec::new(),
            constraint: ColumnConstraint::ForeignKey(self.clone()),
            order: Vec::new(),
            auto_increment: false,
        };
        self.emit_with(writer, &owner, true)
    }
}

impl SqlEmit for ExclusionElement {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expression.emit(writer)?;
        writer.sql(" WITH ");
        self.operator.emit_operator(writer)
    }
}

impl ConstraintExtras {
    fn emit_for(
        &self,
        writer: &mut SqlWriter,
        constraint: &ColumnConstraint,
    ) -> Result<(), DeparseError> {
        let is_unique_or_primary = constraint.is_primary_or_unique();
        let is_foreign = constraint.is_foreign_key();
        let is_check = constraint.is_check();
        let is_exclusion = constraint.is_exclusion();

        if !self.include.is_empty() {
            writer.postgres_only("INCLUDE")?;
            if !is_unique_or_primary && !is_exclusion {
                return writer.invalid("INCLUDE", "INCLUDE requires an index-backed constraint");
            }
            writer.sql(" INCLUDE (");
            writer.names(&self.include, ", ")?;
            writer.sql(")");
        }
        if !self.options.is_empty() {
            writer.postgres_only("constraint index options")?;
            if is_unique_or_primary || is_exclusion {
                writer.sql(" WITH (");
                writer.list(&self.options, ", ")?;
                writer.sql(")");
            } else if constraint.is_identity() {
                // Identity sequence options use a distinct grammar from index options.
            } else {
                return writer.invalid(
                    "constraint options",
                    "options are not legal for this constraint",
                );
            }
        }
        if self.tablespace.is_some() {
            writer.postgres_only("constraint TABLESPACE")?;
            if !is_unique_or_primary && !is_exclusion {
                return writer.invalid(
                    "constraint TABLESPACE",
                    "TABLESPACE requires an index-backed constraint",
                );
            }
            if self.index_name.is_some() {
                return writer.invalid(
                    "constraint TABLESPACE",
                    "an existing index cannot be assigned a new tablespace",
                );
            }
            writer.sql(" USING INDEX TABLESPACE ");
            writer.ident(self.tablespace.as_deref().unwrap_or_default())?;
        }
        if let Some(predicate) = &self.predicate {
            writer.postgres_only("constraint predicate")?;
            if !is_exclusion {
                return writer.invalid(
                    "constraint predicate",
                    "only exclusion constraints support a predicate",
                );
            }
            writer.sql(" WHERE (");
            predicate.emit(writer)?;
            writer.sql(")");
        }
        if self.no_inherit && !is_check {
            return writer.invalid(
                "NO INHERIT",
                "NO INHERIT is valid only for CHECK constraints",
            );
        }
        if self.skip_validation {
            writer.postgres_only("NOT VALID")?;
            if !is_check && !is_foreign {
                return writer.invalid(
                    "NOT VALID",
                    "NOT VALID is valid only for CHECK and FOREIGN KEY constraints",
                );
            }
            writer.sql(" NOT VALID");
        }
        if self.initially_deferred && !self.deferrable {
            return writer.invalid(
                "constraint deferrability",
                "INITIALLY DEFERRED requires DEFERRABLE",
            );
        }
        if self.nulls_not_distinct && !constraint.is_unique() {
            return writer.invalid(
                "NULLS NOT DISTINCT",
                "NULLS NOT DISTINCT is valid only for UNIQUE constraints",
            );
        }
        if self.deferrable || self.initially_deferred {
            writer.postgres_only("constraint deferrability")?;
            if !constraint.supports_deferrability() {
                return writer.invalid(
                    "constraint deferrability",
                    "this constraint cannot be deferrable",
                );
            }
        }
        if self.deferrable {
            writer.sql(" DEFERRABLE");
        }
        if self.initially_deferred {
            writer.sql(" INITIALLY DEFERRED");
        }
        Ok(())
    }
}

impl SqlEmit for ConflictClause {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if writer.is_postgres() {
            return writer.invalid(
                "ON CONFLICT clause",
                "SQLite-only constraint conflict clause",
            );
        }
        writer.sql(match self {
            Self::Abort => "ABORT",
            Self::Fail => "FAIL",
            Self::Ignore => "IGNORE",
            Self::Replace => "REPLACE",
            Self::Rollback => "ROLLBACK",
        });
        Ok(())
    }
}

impl SqlEmit for Deferrability {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.is_initially_deferred() && !self.is_deferrable() {
            return writer.invalid(
                "constraint deferrability",
                "INITIALLY DEFERRED requires DEFERRABLE",
            );
        }
        writer.sql(if self.is_deferrable() {
            "DEFERRABLE"
        } else {
            "NOT DEFERRABLE"
        });
        if self.is_initially_deferred() {
            writer.sql(" INITIALLY DEFERRED");
        }
        Ok(())
    }
}

impl SqlEmit for ForeignKeyMatchType {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql(match self {
            Self::Simple => "SIMPLE",
            Self::Full => "FULL",
            Self::Partial => "PARTIAL",
        });
        Ok(())
    }
}

impl SqlEmit for ForeignKeyAction {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql(match self {
            Self::NoAction => "NO ACTION",
            Self::Restrict => "RESTRICT",
            Self::SetNull => "SET NULL",
            Self::SetDefault => "SET DEFAULT",
            Self::Cascade => "CASCADE",
        });
        Ok(())
    }
}

impl SqlEmit for IdentityColumnMode {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("identity column")?;
        writer.sql(match self {
            Self::Always => "GENERATED ALWAYS AS IDENTITY",
            Self::ByDefault => "GENERATED BY DEFAULT AS IDENTITY",
        });
        Ok(())
    }
}

impl SqlEmit for GeneratedColumnMode {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if writer.is_postgres() && matches!(self, Self::Virtual) {
            return writer.invalid(
                "generated column",
                "PostgreSQL generated columns must be STORED",
            );
        }
        writer.sql(match self {
            Self::Stored => "STORED",
            Self::Virtual => "VIRTUAL",
        });
        Ok(())
    }
}

fn emit_identity_option(option: &SyntaxOption, writer: &mut SqlWriter) -> Result<(), DeparseError> {
    if option.namespace.is_some() {
        return writer.invalid(
            "sequence option",
            "namespaced sequence options are not supported",
        );
    }
    match option.name.as_str() {
        "as" => emit_required_sequence_type(option, writer),
        "start" => emit_required_sequence_option(option, writer, "START WITH"),
        "increment" => emit_required_sequence_option(option, writer, "INCREMENT BY"),
        "cache" => emit_required_sequence_option(option, writer, "CACHE"),
        "minvalue" => emit_optional_sequence_option(option, writer, "MINVALUE", "NO MINVALUE"),
        "maxvalue" => emit_optional_sequence_option(option, writer, "MAXVALUE", "NO MAXVALUE"),
        "restart" => emit_optional_sequence_option(option, writer, "RESTART WITH", "RESTART"),
        "cycle" => emit_cycle_option(option, writer),
        "owned_by" => emit_owned_by_option(option, writer),
        _ => writer.invalid("sequence option", "unknown identity sequence option"),
    }
}

fn emit_identity_options(
    options: &[SyntaxOption],
    writer: &mut SqlWriter,
) -> Result<(), DeparseError> {
    if options.is_empty() {
        return Ok(());
    }
    writer.sql(" (");
    for (index, option) in options.iter().enumerate() {
        if index != 0 {
            writer.sql(" ");
        }
        emit_identity_option(option, writer)?;
    }
    writer.sql(")");
    Ok(())
}

fn emit_required_sequence_type(
    option: &SyntaxOption,
    writer: &mut SqlWriter,
) -> Result<(), DeparseError> {
    let Some(SyntaxValue::Type(type_name)) = &option.value else {
        return writer.invalid("AS sequence type", "a type name is required");
    };
    writer.sql("AS ");
    type_name.emit(writer)
}

fn emit_required_sequence_option(
    option: &SyntaxOption,
    writer: &mut SqlWriter,
    keyword: &str,
) -> Result<(), DeparseError> {
    let Some(value) = &option.value else {
        return writer.invalid("sequence option", "sequence option requires a value");
    };
    writer.sql(keyword);
    writer.sql(" ");
    emit_sequence_value(value, writer)
}

fn emit_optional_sequence_option(
    option: &SyntaxOption,
    writer: &mut SqlWriter,
    with_value: &str,
    without_value: &str,
) -> Result<(), DeparseError> {
    if let Some(value) = &option.value {
        writer.sql(with_value);
        writer.sql(" ");
        emit_sequence_value(value, writer)
    } else {
        writer.sql(without_value);
        Ok(())
    }
}

fn emit_cycle_option(option: &SyntaxOption, writer: &mut SqlWriter) -> Result<(), DeparseError> {
    let cycle = match &option.value {
        None => true,
        Some(SyntaxValue::Expression(crate::ast::Expr::Literal(crate::ast::Literal::Boolean(
            cycle,
        )))) => *cycle,
        _ => return writer.invalid("sequence option", "CYCLE option requires a Boolean value"),
    };
    writer.sql(if cycle { "CYCLE" } else { "NO CYCLE" });
    Ok(())
}

fn emit_owned_by_option(option: &SyntaxOption, writer: &mut SqlWriter) -> Result<(), DeparseError> {
    let Some(SyntaxValue::Expression(value)) = option.value.as_ref() else {
        return writer.invalid("OWNED BY", "an ownership target is required");
    };
    writer.sql("OWNED BY ");
    match value {
        crate::ast::Expr::List(values) => writer.list(values, "."),
        crate::ast::Expr::Column(name) => name.emit(writer),
        crate::ast::Expr::Literal(crate::ast::Literal::Keyword(keyword))
            if keyword.eq_ignore_ascii_case("none") =>
        {
            writer.sql("NONE");
            Ok(())
        }
        _ => value.emit(writer),
    }
}

fn emit_sequence_value(value: &SyntaxValue, writer: &mut SqlWriter) -> Result<(), DeparseError> {
    match value {
        SyntaxValue::Expression(expression) => expression.emit(writer),
        SyntaxValue::Type(_) => {
            writer.invalid("sequence option", "sequence option value cannot be a type")
        }
    }
}

fn emit_conflict_clause(
    conflict: Option<&ConflictClause>,
    writer: &mut SqlWriter,
) -> Result<(), DeparseError> {
    if let Some(conflict) = conflict {
        writer.sql(" ON CONFLICT ");
        <ConflictClause as SqlEmit>::emit(conflict, writer)?;
    }
    Ok(())
}

fn emit_keys(owner: &TableConstraint, writer: &mut SqlWriter) -> Result<(), DeparseError> {
    if !owner.order.is_empty() {
        writer.list(&owner.order, ", ")
    } else {
        writer.names(&owner.keys, ", ")
    }
}

fn emit_inline_primary_order(
    owner: &TableConstraint,
    writer: &mut SqlWriter,
) -> Result<(), DeparseError> {
    if owner.order.len() > 1 {
        return writer.invalid(
            "column PRIMARY KEY",
            "inline primary key has more than one sort item",
        );
    }
    if let Some(order) = owner.order.first() {
        if order.using.is_some() || order.nulls_first.is_some() {
            return writer.invalid(
                "column PRIMARY KEY",
                "inline primary key has unsupported ordering options",
            );
        }
        if let Some(descending) = order.descending {
            writer.sql(if descending { " DESC" } else { " ASC" });
        }
    }
    Ok(())
}

#[cfg(test)]
mod test_constraint_emit {
    use super::{ColumnConstraint, ConstraintExtras, SqlEmit, SqlWriter, TableConstraint};
    use crate::ast::{
        Column, Dialect, Expr, ForeignReference, Literal, Name, SyntaxOption, SyntaxValue, TypeName,
    };
    use crate::sem::{
        ConflictClause, Deferrability, ForeignKeyAction, GeneratedColumnMode, IdentityColumnMode,
    };

    fn render<T: SqlEmit>(value: &T, dialect: Dialect) -> Result<String, super::DeparseError> {
        let mut writer = SqlWriter::new(dialect);
        value.emit(&mut writer)?;
        Ok(writer.sql)
    }

    fn constraint(
        name: Option<&str>,
        keys: &[&str],
        constraint: ColumnConstraint,
    ) -> TableConstraint {
        TableConstraint {
            extras: ConstraintExtras::default(),
            name: name.map(str::to_owned),
            keys: keys.iter().map(|key| (*key).to_owned()).collect(),
            constraint,
            order: Vec::new(),
            auto_increment: false,
        }
    }

    #[rstest::rstest]
    #[case::default(
        "DEFAULT (7)",
        ColumnConstraint::Default(Expr::Literal(Literal::Integer(7)))
    )]
    #[case::check(
        "CHECK (7)",
        ColumnConstraint::Check(Expr::Literal(Literal::Integer(7)))
    )]
    #[case::not_null("NOT NULL", ColumnConstraint::Nullability(true, None, None))]
    fn emits_column_constraint_core(#[case] expected: &str, #[case] value: ColumnConstraint) {
        let table_constraint = constraint(None, &[], value);
        let mut writer = SqlWriter::new(Dialect::postgres());
        table_constraint.emit_as_column(&mut writer).unwrap();
        assert_eq!(writer.sql, expected);
    }

    #[test]
    fn emits_named_table_unique_keys_quoted_and_in_order() {
        let value = constraint(
            Some("unique constraint"),
            &["second", "first"],
            ColumnConstraint::Unique(None),
        );
        assert_eq!(
            render(&value, Dialect::postgres()).unwrap(),
            "CONSTRAINT \"unique constraint\" UNIQUE (\"second\", \"first\")"
        );
    }

    #[test]
    fn emits_inline_sqlite_autoincrement_primary_key() {
        let mut value = constraint(None, &[], ColumnConstraint::PrimaryKey(None, None));
        value.auto_increment = true;
        let mut writer = SqlWriter::new(Dialect::sqlite());
        value.emit_as_column(&mut writer).unwrap();
        assert_eq!(writer.sql, "PRIMARY KEY AUTOINCREMENT");
    }

    #[test]
    fn emits_column_type_and_named_constraints() {
        let column = Column {
            collation: Name::default(),
            compression: None,
            storage: None,
            name: "value".to_owned(),
            type_name: Some(TypeName {
                name: Name::from_parts(&["INTEGER".to_owned()]),
                modifiers: Vec::new(),
                array_bounds: Vec::new(),
                set_of: false,
                percent_type: false,
                sqlite_name_snapshot: None,
                sqlite_parts: Vec::new(),
                sqlite_modifiers: None,
                location: None,
            }),
            constraints: vec![constraint(
                Some("positive"),
                &[],
                ColumnConstraint::Check(Expr::Literal(Literal::Integer(1))),
            )],
            not_null: None,
        };
        assert_eq!(
            render(&column, Dialect::postgres()).unwrap(),
            "\"value\" \"INTEGER\" CONSTRAINT \"positive\" CHECK (1)"
        );
    }

    #[test]
    fn postgres_column_storage_precedes_compression_after_parse() {
        let sql = "CREATE TABLE t (v text STORAGE EXTENDED COMPRESSION pglz)";
        let emitted = crate::parse_statements(sql, Dialect::postgres())
            .next()
            .expect("one statement")
            .expect(sql)
            .deparse_sql(Dialect::postgres())
            .unwrap();
        let storage = emitted.find("STORAGE EXTENDED").expect(&emitted);
        let compression = emitted.find("COMPRESSION").expect(&emitted);
        assert!(storage < compression, "{emitted}");

        let reparsed = crate::parse_statements(&emitted, Dialect::postgres())
            .next()
            .expect("one statement after deparse")
            .expect(&emitted)
            .deparse_sql(Dialect::postgres())
            .unwrap();
        assert_eq!(emitted, reparsed);
    }

    #[test]
    fn emits_typed_table_column_with_options_and_quoted_collation() {
        let column = Column {
            collation: Name::from_parts(&["sort\"order".to_owned()]),
            compression: None,
            storage: None,
            name: "value".to_owned(),
            type_name: None,
            constraints: Vec::new(),
            not_null: None,
        };
        assert_eq!(
            render(&column, Dialect::postgres()).unwrap(),
            "\"value\" WITH OPTIONS COLLATE \"sort\"\"order\""
        );
    }

    #[test]
    fn emits_unique_index_parameters_and_tablespace() {
        let mut value = constraint(None, &["key"], ColumnConstraint::Unique(None));
        value.extras.options.push(SyntaxOption {
            namespace: None,
            name: "fillfactor".to_owned(),
            value: Some(SyntaxValue::Expression(Expr::Literal(Literal::Integer(70)))),
        });
        value.extras.tablespace = Some("fast space".to_owned());
        assert_eq!(
            render(&value, Dialect::postgres()).unwrap(),
            "UNIQUE (\"key\") WITH (\"fillfactor\" = 70) USING INDEX TABLESPACE \"fast space\""
        );
    }

    #[test]
    fn emits_identity_sequence_options_with_sequence_grammar() {
        let mut value = constraint(
            None,
            &[],
            ColumnConstraint::Identity(IdentityColumnMode::ByDefault),
        );
        value.extras.options = vec![
            SyntaxOption {
                namespace: None,
                name: "start".to_owned(),
                value: Some(SyntaxValue::Expression(Expr::Literal(Literal::Integer(8)))),
            },
            SyntaxOption {
                namespace: None,
                name: "cycle".to_owned(),
                value: Some(SyntaxValue::Expression(Expr::Literal(Literal::Boolean(
                    false,
                )))),
            },
        ];
        let mut writer = SqlWriter::new(Dialect::postgres());
        value.emit_as_column(&mut writer).unwrap();
        assert_eq!(
            writer.sql,
            "GENERATED BY DEFAULT AS IDENTITY (START WITH 8 NO CYCLE)"
        );
    }

    #[test]
    fn emits_foreign_key_actions_and_deferrability() {
        let reference = ForeignReference {
            table: Name::from_parts(&["public".to_owned(), "parent".to_owned()]),
            columns: vec!["id".to_owned()],
            match_type: None,
            delete_action: Some(ForeignKeyAction::Cascade),
            update_action: None,
            deferrability: Some(Deferrability::new(true, true)),
            validity: None,
            ignored_matches: Vec::new(),
        };
        let value = constraint(None, &[], ColumnConstraint::ForeignKey(reference));
        let mut writer = SqlWriter::new(Dialect::sqlite());
        value.emit_as_column(&mut writer).unwrap();
        assert_eq!(
            writer.sql,
            "REFERENCES \"public\".\"parent\" (\"id\") ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED"
        );
    }

    #[test]
    fn emits_sqlite_conflict_action_and_rejects_it_for_postgres() {
        let value = constraint(
            None,
            &[],
            ColumnConstraint::Nullability(true, Some(ConflictClause::Ignore), None),
        );
        let mut sqlite = SqlWriter::new(Dialect::sqlite());
        value.emit_as_column(&mut sqlite).unwrap();
        assert_eq!(sqlite.sql, "NOT NULL ON CONFLICT IGNORE");
        let mut postgres = SqlWriter::new(Dialect::postgres());
        let error = value.emit_as_column(&mut postgres).unwrap_err();
        assert!(error.is_invalid_syntax(), "{error}");
        assert_eq!(error.node(), "ON CONFLICT clause", "{error}");
    }

    #[test]
    fn rejects_virtual_generated_columns_for_postgres() {
        let error = render(&GeneratedColumnMode::Virtual, Dialect::postgres()).unwrap_err();
        assert!(error.is_invalid_syntax(), "{error}");
        assert_eq!(error.node(), "generated column", "{error}");
    }
}
