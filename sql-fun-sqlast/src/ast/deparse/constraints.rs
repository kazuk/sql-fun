use crate::ast::{
    Column, ColumnConstraint, ConstraintExtras, ExclusionElement, Expr as AstExpr, FdwOption,
    ForeignReference, Literal as AstLiteral, SyntaxOption, SyntaxValue, TableConstraint,
    deparse::{DeparseError, SqlEmit, SqlWriter},
};
use crate::syntax_options::{
    ConflictClause, Deferrability, Deferrable, ForeignKeyAction, ForeignKeyMatchType,
    GeneratedColumnMode, IdentityColumnMode,
};

impl SqlEmit for Column {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.emit_with_fdw_options(writer, &[])
    }
}

impl Column {
    pub(in crate::ast) fn emit_with_fdw_options(
        &self,
        writer: &mut SqlWriter,
        options: &[FdwOption],
    ) -> Result<(), DeparseError> {
        writer.ident(&self.name)?;
        if let Some(type_name) = &self.type_name {
            writer.sql(" ");
            type_name.emit(writer)?;
        } else if writer.is_postgres() {
            writer.sql(" WITH OPTIONS");
        }
        FdwOption::emit_options(options, writer)?;
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

    fn emit_keys(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if !self.order.is_empty() {
            writer.list(&self.order, ", ")
        } else {
            writer.names(&self.keys, ", ")?;
            if self.extras.without_overlaps {
                writer.sql(" WITHOUT OVERLAPS");
            }
            Ok(())
        }
    }

    fn emit_inline_primary_order(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.order.len() > 1 {
            return writer.invalid(
                "column PRIMARY KEY",
                "inline primary key has more than one sort item",
            );
        }
        if let Some(order) = self.order.first() {
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
            Self::Enforcement(enforced) => writer.sql(if *enforced {
                "ENFORCED"
            } else {
                "NOT ENFORCED"
            }),
            Self::Nullability(true, _, _) if !column_context => {
                writer.sql("NOT NULL ");
                writer.names(&owner.keys, ", ")?;
            }
            Self::Nullability(not_null, conflict, _validity) => {
                Self::emit_nullability(*not_null, conflict.as_ref(), column_context, writer)?;
            }
            Self::Default(expression) => Self::emit_default(expression, column_context, writer)?,
            Self::Check(expression) => Self::emit_check(expression, owner, writer)?,
            Self::PrimaryKey(conflict, deferrable) => {
                Self::emit_primary_key(
                    conflict.as_ref(),
                    deferrable.as_ref(),
                    owner,
                    column_context,
                    writer,
                )?;
            }
            Self::Unique(conflict) => {
                Self::emit_unique(conflict.as_ref(), owner, column_context, writer)?;
            }
            Self::Identity(mode) => Self::emit_identity(mode, owner, column_context, writer)?,
            Self::Generated(expression, mode) => {
                Self::emit_generated(expression, mode.as_ref(), column_context, writer)?;
            }
            Self::ForeignKey(reference) => {
                reference.emit_with(writer, owner, column_context)?;
            }
            Self::Exclusion => Self::emit_exclusion(owner, column_context, writer)?,
            Self::Deferrability(deferrable) => Self::emit_deferrability(*deferrable, writer)?,
            Self::Deferred => Self::emit_deferred(writer)?,
            Self::Immediate => Self::emit_immediate(writer)?,
            Self::Collate(name) => Self::emit_collation(name, column_context, writer)?,
        }
        Ok(())
    }

    fn emit_nullability(
        not_null: bool,
        conflict: Option<&ConflictClause>,
        column_context: bool,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        if !column_context {
            return writer.invalid(
                "nullability constraint",
                "nullability is a column constraint",
            );
        }
        writer.sql(if not_null { "NOT NULL" } else { "NULL" });
        ConflictClause::emit_constraint_clause(conflict, writer)
    }

    fn emit_default(
        expression: &AstExpr,
        column_context: bool,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        if !column_context {
            return writer.invalid("DEFAULT constraint", "DEFAULT is a column constraint");
        }
        writer.sql("DEFAULT (");
        expression.emit(writer)?;
        writer.sql(")");
        Ok(())
    }

    fn emit_check(
        expression: &AstExpr,
        owner: &TableConstraint,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.sql("CHECK (");
        expression.emit(writer)?;
        writer.sql(")");
        if owner.extras.no_inherit {
            writer.postgres_only("CHECK NO INHERIT")?;
            writer.sql(" NO INHERIT");
        }
        Ok(())
    }

    fn emit_primary_key(
        conflict: Option<&ConflictClause>,
        deferrable: Option<&Deferrable>,
        owner: &TableConstraint,
        column_context: bool,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.sql("PRIMARY KEY");
        if column_context {
            if !owner.keys.is_empty() {
                return writer.invalid(
                    "column PRIMARY KEY",
                    "an inline primary key cannot have a key list",
                );
            }
            owner.emit_inline_primary_order(writer)?;
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
            owner.emit_keys(writer)?;
            writer.sql(")");
        } else {
            return writer.invalid("PRIMARY KEY", "table primary key requires at least one key");
        }
        ConflictClause::emit_constraint_clause(conflict, writer)?;
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
        Ok(())
    }

    fn emit_unique(
        conflict: Option<&ConflictClause>,
        owner: &TableConstraint,
        column_context: bool,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
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
            owner.emit_keys(writer)?;
            writer.sql(")");
        } else {
            return writer.invalid(
                "UNIQUE",
                "table UNIQUE constraint requires at least one key",
            );
        }
        ConflictClause::emit_constraint_clause(conflict, writer)
    }

    fn emit_identity(
        mode: &IdentityColumnMode,
        owner: &TableConstraint,
        column_context: bool,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        if !column_context {
            return writer.invalid("identity column", "identity is a column constraint");
        }
        mode.emit(writer)?;
        owner.extras.emit_identity_options(writer)
    }

    fn emit_generated(
        expression: &AstExpr,
        mode: Option<&GeneratedColumnMode>,
        column_context: bool,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
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
        Ok(())
    }

    fn emit_exclusion(
        owner: &TableConstraint,
        column_context: bool,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
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
        Ok(())
    }

    fn emit_deferrability(deferrable: bool, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("constraint deferrability")?;
        writer.sql(if deferrable {
            "DEFERRABLE"
        } else {
            "NOT DEFERRABLE"
        });
        Ok(())
    }

    fn emit_deferred(writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("constraint deferrability")?;
        writer.sql("INITIALLY DEFERRED");
        Ok(())
    }

    fn emit_immediate(writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("constraint deferrability")?;
        writer.sql("INITIALLY IMMEDIATE");
        Ok(())
    }

    fn emit_collation(
        name: &str,
        column_context: bool,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        if !column_context {
            return writer.invalid("COLLATE constraint", "COLLATE is a column constraint");
        }
        writer.sql("COLLATE ");
        writer.ident(name)
    }
}

impl ForeignReference {
    fn emit_period_keys(
        keys: &[String],
        period: bool,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        for (index, key) in keys.iter().enumerate() {
            if index != 0 {
                writer.sql(", ");
            }
            if period && index + 1 == keys.len() {
                writer.sql("PERIOD ");
            }
            writer.ident(key)?;
        }
        Ok(())
    }

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
            Self::emit_period_keys(&owner.keys, owner.extras.fk_with_period, writer)?;
            writer.sql(") ");
        }
        writer.sql("REFERENCES ");
        self.table.emit(writer)?;
        if !self.columns.is_empty() {
            writer.sql(" (");
            Self::emit_period_keys(&self.columns, owner.extras.pk_with_period, writer)?;
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
        self.emit_delete_action(&owner.extras.delete_columns, writer)?;
        if !(owner.extras.deferrable || owner.extras.initially_deferred)
            && let Some(deferrability) = &self.deferrability
        {
            writer.sql(" ");
            deferrability.emit(writer)?;
        }
        Ok(())
    }

    fn emit_delete_action(
        &self,
        columns: &[String],
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        let Some(action) = &self.delete_action else {
            return if columns.is_empty() {
                Ok(())
            } else {
                writer.invalid(
                    "ON DELETE column list",
                    "a column list requires an ON DELETE SET NULL or SET DEFAULT action",
                )
            };
        };
        writer.sql(" ON DELETE ");
        action.emit(writer)?;
        if columns.is_empty() {
            return Ok(());
        }
        writer.postgres_only("ON DELETE SET NULL/DEFAULT column list")?;
        if !action.supports_delete_column_list() {
            return writer.invalid(
                "ON DELETE column list",
                "a column list is valid only with SET NULL or SET DEFAULT",
            );
        }
        writer.sql(" (");
        writer.names(columns, ", ")?;
        writer.sql(")");
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
        self.emit_index_options(writer, constraint)?;
        self.emit_predicate(writer, constraint)?;
        self.emit_validation_options(writer, constraint)?;
        if constraint.is_nullability() && self.no_inherit {
            writer.sql(" NO INHERIT");
        }
        if let Some(enforced) = self.enforced {
            writer.postgres_only("constraint enforcement")?;
            writer.sql(if enforced {
                " ENFORCED"
            } else {
                " NOT ENFORCED"
            });
        }
        self.emit_deferrability(writer, constraint)
    }

    fn emit_index_options(
        &self,
        writer: &mut SqlWriter,
        constraint: &ColumnConstraint,
    ) -> Result<(), DeparseError> {
        let is_unique_or_primary = constraint.is_primary_or_unique();
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
        Ok(())
    }

    fn emit_predicate(
        &self,
        writer: &mut SqlWriter,
        constraint: &ColumnConstraint,
    ) -> Result<(), DeparseError> {
        if let Some(predicate) = &self.predicate {
            writer.postgres_only("constraint predicate")?;
            if !constraint.is_exclusion() {
                return writer.invalid(
                    "constraint predicate",
                    "only exclusion constraints support a predicate",
                );
            }
            writer.sql(" WHERE (");
            predicate.emit(writer)?;
            writer.sql(")");
        }
        Ok(())
    }

    fn emit_validation_options(
        &self,
        writer: &mut SqlWriter,
        constraint: &ColumnConstraint,
    ) -> Result<(), DeparseError> {
        if self.no_inherit && !constraint.is_check() && !constraint.is_nullability() {
            return writer.invalid(
                "NO INHERIT",
                "NO INHERIT is valid only for CHECK constraints",
            );
        }
        if self.skip_validation {
            writer.postgres_only("NOT VALID")?;
            if !constraint.is_check()
                && !constraint.is_foreign_key()
                && !constraint.is_nullability()
            {
                return writer.invalid(
                    "NOT VALID",
                    "NOT VALID is valid only for CHECK and FOREIGN KEY constraints",
                );
            }
            writer.sql(" NOT VALID");
        }
        Ok(())
    }

    fn emit_deferrability(
        &self,
        writer: &mut SqlWriter,
        constraint: &ColumnConstraint,
    ) -> Result<(), DeparseError> {
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

    fn emit_identity_options(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.options.is_empty() {
            return Ok(());
        }
        writer.sql(" (");
        for (index, option) in self.options.iter().enumerate() {
            if index != 0 {
                writer.sql(" ");
            }
            option.emit_identity_option(writer)?;
        }
        writer.sql(")");
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

impl ForeignKeyAction {
    const fn supports_delete_column_list(&self) -> bool {
        matches!(self, Self::SetNull | Self::SetDefault)
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
        writer.sql(match self {
            Self::Stored => "STORED",
            Self::Virtual => "VIRTUAL",
        });
        Ok(())
    }
}

impl SyntaxOption {
    fn emit_identity_option(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.namespace.is_some() {
            return writer.invalid(
                "sequence option",
                "namespaced sequence options are not supported",
            );
        }
        match self.name.as_str() {
            "as" => self.emit_identity_type(writer),
            "start" => self.emit_required_identity_option("START WITH", writer),
            "increment" => self.emit_required_identity_option("INCREMENT BY", writer),
            "cache" => self.emit_required_identity_option("CACHE", writer),
            "minvalue" => self.emit_optional_identity_option("MINVALUE", "NO MINVALUE", writer),
            "maxvalue" => self.emit_optional_identity_option("MAXVALUE", "NO MAXVALUE", writer),
            "restart" => self.emit_optional_identity_option("RESTART WITH", "RESTART", writer),
            "cycle" => self.emit_identity_cycle(writer),
            "owned_by" => self.emit_identity_owned_by(writer),
            _ => writer.invalid("sequence option", "unknown identity sequence option"),
        }
    }

    fn emit_identity_type(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let Some(SyntaxValue::Type(type_name)) = &self.value else {
            return writer.invalid("AS sequence type", "a type name is required");
        };
        writer.sql("AS ");
        type_name.emit(writer)
    }

    fn emit_required_identity_option(
        &self,
        keyword: &str,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        let Some(value) = &self.value else {
            return writer.invalid("sequence option", "sequence option requires a value");
        };
        writer.sql(keyword);
        writer.sql(" ");
        value.emit_identity_sequence_value(writer)
    }

    fn emit_optional_identity_option(
        &self,
        with_value: &str,
        without_value: &str,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        if let Some(value) = &self.value {
            writer.sql(with_value);
            writer.sql(" ");
            value.emit_identity_sequence_value(writer)
        } else {
            writer.sql(without_value);
            Ok(())
        }
    }

    fn emit_identity_cycle(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let cycle = match &self.value {
            None => true,
            Some(SyntaxValue::Expression(AstExpr::Literal(AstLiteral::Boolean(cycle)))) => *cycle,
            _ => return writer.invalid("sequence option", "CYCLE option requires a Boolean value"),
        };
        writer.sql(if cycle { "CYCLE" } else { "NO CYCLE" });
        Ok(())
    }

    fn emit_identity_owned_by(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let Some(SyntaxValue::Expression(value)) = self.value.as_ref() else {
            return writer.invalid("OWNED BY", "an ownership target is required");
        };
        writer.sql("OWNED BY ");
        match value {
            AstExpr::List(values) => writer.list(values, "."),
            AstExpr::Column(name) => name.emit(writer),
            AstExpr::Literal(AstLiteral::Keyword(keyword))
                if keyword.eq_ignore_ascii_case("none") =>
            {
                writer.sql("NONE");
                Ok(())
            }
            _ => value.emit(writer),
        }
    }
}

impl SyntaxValue {
    fn emit_identity_sequence_value(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Expression(expression) => expression.emit(writer),
            Self::Type(_) => {
                writer.invalid("sequence option", "sequence option value cannot be a type")
            }
        }
    }
}

impl ConflictClause {
    fn emit_constraint_clause(
        conflict: Option<&Self>,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        if let Some(conflict) = conflict {
            writer.sql(" ON CONFLICT ");
            conflict.emit(writer)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_constraint_emit {
    use crate::syntax_options::{
        ConflictClause, Deferrability, ForeignKeyAction, GeneratedColumnMode, IdentityColumnMode,
    };
    use crate::test_helpers::deparse::render;
    use crate::test_helpers::deparse_constraints::constraint;

    use crate::{ast::StatementNode, parse_statements, test_helpers::assert_postgres_structure};

    use crate::ast::{
        Column, Dialect, Expr, ForeignReference, Literal, Name, SyntaxOption, SyntaxValue,
        TypeName,
        deparse::constraints::{ColumnConstraint, SqlWriter},
    };

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
            type_mod: None,
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
        let emitted = parse_statements(sql, Dialect::postgres())
            .next()
            .expect("one statement")
            .expect(sql)
            .deparse_sql(Dialect::postgres())
            .unwrap();
        let storage = emitted.find("STORAGE EXTENDED").expect(&emitted);
        let compression = emitted.find("COMPRESSION").expect(&emitted);
        assert!(storage < compression, "{emitted}");

        let reparsed = parse_statements(&emitted, Dialect::postgres())
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
            type_mod: None,
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

    #[rstest::rstest]
    #[case::set_null(
        r#"CREATE TABLE child (a integer, b integer, FOREIGN KEY (a, b) REFERENCES parent (a, b) ON DELETE SET NULL (a, b))"#,
        r#"ON DELETE SET NULL ("a", "b")"#
    )]
    #[case::set_default(
        r#"CREATE TABLE child (a integer, b integer, FOREIGN KEY (a, b) REFERENCES parent (a, b) ON DELETE SET DEFAULT (a))"#,
        r#"ON DELETE SET DEFAULT ("a")"#
    )]
    fn postgres_frontend_retains_on_delete_column_lists(#[case] sql: &str, #[case] expected: &str) {
        assert_postgres_structure(sql);
        let first = parse_statements(sql, Dialect::postgres())
            .next()
            .expect("one statement")
            .expect(sql)
            .deparse_sql(Dialect::postgres())
            .unwrap();
        assert!(first.contains(expected), "{sql} => {first}");

        let second = parse_statements(&first, Dialect::postgres())
            .next()
            .expect("one statement after deparse")
            .expect(&first)
            .deparse_sql(Dialect::postgres())
            .unwrap();
        assert_eq!(first, second, "{sql} => {first}");
    }

    #[rstest::rstest]
    #[case::cascade(Some(ForeignKeyAction::Cascade))]
    #[case::missing_action(None)]
    fn rejects_on_delete_column_lists_with_other_actions_as_invalid_syntax(
        #[case] action: Option<ForeignKeyAction>,
    ) {
        let mut statement = parse_statements(
            "CREATE TABLE child (parent_id integer, FOREIGN KEY (parent_id) REFERENCES parent(id) ON DELETE SET DEFAULT (parent_id))",
            Dialect::postgres(),
        )
        .next()
        .unwrap()
        .unwrap();
        let StatementNode::Table(table) = &mut statement.node else {
            panic!("table fixture");
        };
        let ColumnConstraint::ForeignKey(reference) = &mut table.constraints[0].constraint else {
            panic!("foreign key fixture");
        };
        reference.delete_action = action;

        let error = statement.deparse_sql(Dialect::postgres()).unwrap_err();
        assert!(error.is_invalid_syntax(), "{error}");
        assert_eq!(
            error.node(),
            "CREATE TABLE/ON DELETE column list",
            "{error}"
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
    fn emits_virtual_generated_columns_for_postgres() {
        assert_eq!(
            render(&GeneratedColumnMode::Virtual, Dialect::postgres()).unwrap(),
            "VIRTUAL"
        );
    }
}

#[cfg(test)]
mod test_postgres18_constraint_syntax {
    use crate::test_helpers::assert_postgres_structure;

    #[rstest::rstest]
    #[case::virtual_column(
        "CREATE TABLE t (id int, computed int GENERATED ALWAYS AS (id+1) VIRTUAL)"
    )]
    #[case::implicit_virtual_column(
        "CREATE TABLE t (id int, computed int GENERATED ALWAYS AS (id+1))"
    )]
    #[case::temporal_primary(
        "CREATE TABLE t (id int, valid_at daterange, PRIMARY KEY (id, valid_at WITHOUT OVERLAPS))"
    )]
    #[case::temporal_foreign(
        "CREATE TABLE t (id int, valid_at daterange, FOREIGN KEY (id, PERIOD valid_at) REFERENCES p (id, PERIOD valid_at))"
    )]
    #[case::unenforced_table_check("CREATE TABLE t (id int, CHECK (id>0) NOT ENFORCED)")]
    #[case::enforced_column_check("CREATE TABLE t (id int CHECK (id>0) ENFORCED)")]
    #[case::table_not_null("CREATE TABLE t (id int, CONSTRAINT nn NOT NULL id)")]
    #[case::column_not_null_no_inherit("CREATE TABLE t (id int NOT NULL NO INHERIT)")]
    #[case::alter_enforced("ALTER TABLE t ALTER CONSTRAINT check_id ENFORCED")]
    #[case::alter_not_enforced("ALTER TABLE t ALTER CONSTRAINT check_id NOT ENFORCED")]
    #[case::alter_inherit("ALTER TABLE t ALTER CONSTRAINT check_id INHERIT")]
    #[case::alter_no_inherit("ALTER TABLE t ALTER CONSTRAINT check_id NO INHERIT")]
    #[case::domain_enforced("CREATE DOMAIN positive_int AS int CHECK (VALUE>0) ENFORCED")]
    fn preserves_postgres18_attributes(#[case] sql: &str) {
        assert_postgres_structure(sql);
    }
}
