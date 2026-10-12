use super::{DeparseError, SqlEmit, SqlWriter};
use crate::ast::{
    AlterOwner, AlterSequence, Comment, DropTable, EnumType, Expr, Extension, Literal, Name,
    ObjectArgument, ObjectTarget, Role, Sequence, SyntaxOption, SyntaxValue, VariableSet,
    VariableSetKind, View, ViewCheckOption,
};

impl SqlEmit for Extension {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("CREATE EXTENSION")?;
        writer.sql("CREATE EXTENSION");
        if self.if_not_exists {
            writer.sql(" IF NOT EXISTS");
        }
        writer.sql(" ");
        writer.ident(&self.name)?;
        for option in &self.options {
            option.emit_extension_option(writer)?;
        }
        Ok(())
    }
}

impl SyntaxOption {
    fn emit_extension_option(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.namespace.is_some() {
            return writer.unsupported("CREATE EXTENSION option namespace");
        }
        match self.name.as_str() {
            "schema" => {
                writer.sql(" SCHEMA ");
                let Some(SyntaxValue::Expression(Expr::Literal(Literal::String(name)))) =
                    self.value.as_ref()
                else {
                    return writer.invalid("CREATE EXTENSION SCHEMA", "expected a schema name");
                };
                writer.ident(name)
            }
            "new_version" => {
                writer.sql(" VERSION ");
                self.emit_extension_string(writer)
            }
            "cascade" => {
                match self.value.as_ref() {
                    None => writer.sql(" CASCADE"),
                    Some(SyntaxValue::Expression(Expr::Literal(Literal::Boolean(true)))) => {
                        writer.sql(" CASCADE")
                    }
                    Some(SyntaxValue::Expression(Expr::Literal(Literal::Boolean(false)))) => {
                        return writer.invalid(
                            "CREATE EXTENSION CASCADE",
                            "a present CASCADE option must be true",
                        );
                    }
                    _ => {
                        return writer.invalid(
                            "CREATE EXTENSION CASCADE",
                            "expected a boolean cascade option",
                        );
                    }
                }
                Ok(())
            }
            _ => writer.unsupported("CREATE EXTENSION option"),
        }
    }

    fn emit_extension_string(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let Some(SyntaxValue::Expression(Expr::Literal(Literal::String(value)))) =
            self.value.as_ref()
        else {
            return writer.invalid("CREATE EXTENSION option", "expected a string value");
        };
        writer.string(value)
    }
}

impl SqlEmit for VariableSet {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("SET/RESET")?;
        if matches!(self.kind, VariableSetKind::Multi) {
            match self.name.to_ascii_lowercase().as_str() {
                "transaction" => return self.emit_transaction_modes(writer, false),
                "session characteristics" => {
                    return self.emit_transaction_modes(writer, true);
                }
                "transaction snapshot" => return self.emit_transaction_snapshot(writer),
                _ => {}
            }
        }
        self.kind
            .emit(writer, self.is_local, &self.name, &self.arguments)
    }
}

impl VariableSet {
    fn emit_transaction_modes(
        &self,
        writer: &mut SqlWriter,
        session_characteristics: bool,
    ) -> Result<(), DeparseError> {
        if !self.arguments.is_empty() {
            return writer.invalid("SET TRANSACTION", "unexpected positional values");
        }
        let Some(options) = &self.transaction_options else {
            return writer.invalid("SET TRANSACTION", "transaction modes are missing");
        };
        if options.is_empty() {
            return writer.invalid(
                "SET TRANSACTION",
                "at least one transaction mode is required",
            );
        }
        writer.sql("SET");
        if self.is_local {
            writer.sql(" LOCAL");
        }
        if session_characteristics {
            writer.sql(" SESSION CHARACTERISTICS AS TRANSACTION ");
        } else {
            writer.sql(" TRANSACTION ");
        }
        for (index, option) in options.iter().enumerate() {
            if index != 0 {
                writer.sql(", ");
            }
            Self::emit_transaction_mode(option, writer)?;
        }
        Ok(())
    }

    fn emit_transaction_mode(
        option: &SyntaxOption,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        if option.namespace.is_some() {
            return writer.unsupported("transaction mode namespace");
        }
        match option.name.as_str() {
            "transaction_isolation" => {
                writer.sql("ISOLATION LEVEL ");
                let Some(SyntaxValue::Expression(Expr::Literal(Literal::String(level)))) =
                    option.value.as_ref()
                else {
                    return writer.invalid("ISOLATION LEVEL", "expected an isolation level");
                };
                let level = match level.to_ascii_lowercase().as_str() {
                    "serializable" => "SERIALIZABLE",
                    "repeatable read" => "REPEATABLE READ",
                    "read committed" => "READ COMMITTED",
                    "read uncommitted" => "READ UNCOMMITTED",
                    _ => return writer.invalid("ISOLATION LEVEL", "unknown isolation level"),
                };
                writer.sql(level);
                Ok(())
            }
            "transaction_read_only" => {
                let Some(read_only) = Self::transaction_mode_boolean(option) else {
                    return writer.invalid("transaction mode", "expected a boolean value");
                };
                if read_only {
                    writer.sql("READ ONLY");
                } else {
                    writer.sql("READ WRITE");
                }
                Ok(())
            }
            "transaction_deferrable" => {
                let Some(deferrable) = Self::transaction_mode_boolean(option) else {
                    return writer.invalid("transaction mode", "expected a boolean value");
                };
                if deferrable {
                    writer.sql("DEFERRABLE");
                } else {
                    writer.sql("NOT DEFERRABLE");
                }
                Ok(())
            }
            _ => writer.unsupported("transaction mode"),
        }
    }

    fn transaction_mode_boolean(option: &SyntaxOption) -> Option<bool> {
        let Some(SyntaxValue::Expression(Expr::Literal(value))) = option.value.as_ref() else {
            return None;
        };
        match value {
            Literal::Boolean(value) => Some(*value),
            Literal::Integer(0) => Some(false),
            Literal::Integer(1) => Some(true),
            Literal::String(value) => match value.to_ascii_lowercase().as_str() {
                "false" | "off" | "no" | "0" => Some(false),
                "true" | "on" | "yes" | "1" => Some(true),
                _ => None,
            },
            _ => None,
        }
    }

    fn emit_transaction_snapshot(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let [Expr::Literal(Literal::String(snapshot))] = self.arguments.as_slice() else {
            return writer.invalid(
                "SET TRANSACTION SNAPSHOT",
                "exactly one snapshot identifier is required",
            );
        };
        if self.transaction_options.is_some() {
            return writer.invalid("SET TRANSACTION SNAPSHOT", "unexpected transaction modes");
        }
        writer.sql("SET");
        if self.is_local {
            writer.sql(" LOCAL");
        }
        writer.sql(" TRANSACTION SNAPSHOT ");
        writer.string(snapshot)
    }
}

impl VariableSetKind {
    fn emit(
        &self,
        writer: &mut SqlWriter,
        is_local: bool,
        name: &str,
        arguments: &[Expr],
    ) -> Result<(), DeparseError> {
        match self {
            Self::ResetAll => {
                if is_local || !name.is_empty() || !arguments.is_empty() {
                    return writer.invalid("RESET ALL", "RESET ALL cannot have a name or value");
                }
                writer.sql("RESET ALL");
            }
            Self::Reset => {
                if is_local || !arguments.is_empty() {
                    return writer.invalid("RESET", "RESET cannot be local or have a value");
                }
                writer.sql("RESET ");
                Self::emit_setting_name(writer, name)?;
            }
            Self::Value | Self::Default | Self::Current => {
                writer.sql("SET");
                if is_local {
                    writer.sql(" LOCAL");
                }
                writer.sql(" ");
                Self::emit_setting_name(writer, name)?;
                match self {
                    Self::Current => {
                        if !arguments.is_empty() {
                            return writer.invalid(
                                "SET FROM CURRENT",
                                "FROM CURRENT cannot have explicit values",
                            );
                        }
                        writer.sql(" FROM CURRENT");
                    }
                    Self::Default => {
                        if !arguments.is_empty() {
                            return writer
                                .invalid("SET TO DEFAULT", "DEFAULT cannot have explicit values");
                        }
                        writer.sql(" TO DEFAULT");
                    }
                    Self::Value => {
                        if arguments.is_empty() {
                            return writer.invalid("SET", "a setting value is required");
                        }
                        writer.sql(" TO ");
                        writer.list(arguments, ", ")?;
                    }
                    Self::Reset | Self::ResetAll => {
                        return writer.invalid("SET", "RESET modes do not accept setting values");
                    }
                    Self::Multi => return writer.unsupported("SET special mode"),
                }
            }
            Self::Multi => return writer.unsupported("SET special mode"),
        }
        Ok(())
    }

    fn emit_setting_name(writer: &mut SqlWriter, name: &str) -> Result<(), DeparseError> {
        if name.is_empty() {
            return writer.invalid("setting name", "setting name is empty");
        }
        // PostgreSQL's native AST stores the complete GUC name as one string, including
        // dots that may have appeared inside a quoted identifier.
        writer.ident(name)
    }
}

impl SqlEmit for EnumType {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("CREATE TYPE AS ENUM")?;
        if self.values.is_empty() {
            return writer.invalid("CREATE TYPE AS ENUM", "at least one enum label is required");
        }
        writer.sql("CREATE TYPE ");
        self.name.emit(writer)?;
        writer.sql(" AS ENUM (");
        for (index, value) in self.values.iter().enumerate() {
            if index != 0 {
                writer.sql(", ");
            }
            writer.string(value)?;
        }
        writer.sql(")");
        Ok(())
    }
}

impl SqlEmit for Sequence {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("CREATE SEQUENCE")?;
        writer.sql("CREATE SEQUENCE");
        if self.if_not_exists {
            writer.sql(" IF NOT EXISTS");
        }
        writer.sql(" ");
        self.name.emit(writer)?;
        for option in &self.options {
            option.emit_sequence_option(writer)?;
        }
        Ok(())
    }
}

impl SqlEmit for AlterSequence {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("ALTER SEQUENCE")?;
        writer.sql("ALTER SEQUENCE");
        if self.missing_ok {
            writer.sql(" IF EXISTS");
        }
        writer.sql(" ");
        self.name.emit(writer)?;
        for option in &self.options {
            option.emit_sequence_option(writer)?;
        }
        if self.options.is_empty() {
            return writer.invalid("ALTER SEQUENCE", "at least one sequence option is required");
        }
        Ok(())
    }
}

impl SyntaxOption {
    pub(in crate::ast) fn emit_sequence_option(
        &self,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        if self.namespace.is_some() {
            return writer.unsupported("sequence option namespace");
        }
        match self.name.as_str() {
            "as" => {
                writer.sql(" AS ");
                self.emit_required_type(writer)
            }
            "increment" => {
                writer.sql(" INCREMENT BY ");
                self.emit_required_expression(writer)
            }
            "minvalue" => self.emit_sequence_bound(writer, "MINVALUE", "NO MINVALUE"),
            "maxvalue" => self.emit_sequence_bound(writer, "MAXVALUE", "NO MAXVALUE"),
            "start" => {
                writer.sql(" START WITH ");
                self.emit_required_expression(writer)
            }
            "restart" => {
                writer.sql(" RESTART");
                if self.value.is_some() {
                    writer.sql(" WITH ");
                    self.emit_required_expression(writer)?;
                }
                Ok(())
            }
            "cache" => {
                writer.sql(" CACHE ");
                self.emit_required_expression(writer)
            }
            "cycle" => {
                match self.value.as_ref() {
                    None | Some(SyntaxValue::Expression(Expr::Literal(Literal::Boolean(true)))) => {
                        writer.sql(" CYCLE")
                    }
                    Some(SyntaxValue::Expression(Expr::Literal(Literal::Boolean(false)))) => {
                        writer.sql(" NO CYCLE")
                    }
                    _ => return writer.invalid("CYCLE", "expected a boolean cycle option"),
                }
                Ok(())
            }
            "owned_by" => {
                writer.sql(" OWNED BY ");
                self.emit_owned_by(writer)
            }
            _ => writer.unsupported("sequence option"),
        }
    }

    fn emit_sequence_bound(
        &self,
        writer: &mut SqlWriter,
        bound: &str,
        no_bound: &str,
    ) -> Result<(), DeparseError> {
        match self.value.as_ref() {
            Some(SyntaxValue::Expression(value)) => {
                writer.sql(" ");
                writer.sql(bound);
                writer.sql(" ");
                value.emit(writer)
            }
            None => {
                writer.sql(" ");
                writer.sql(no_bound);
                Ok(())
            }
            Some(SyntaxValue::Type(_)) => {
                writer.invalid("sequence bound", "expected a numeric bound")
            }
        }
    }

    fn emit_required_expression(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self.value.as_ref() {
            Some(SyntaxValue::Expression(value)) => value.emit(writer),
            Some(SyntaxValue::Type(_)) => {
                writer.invalid("sequence option", "an expression value is required")
            }
            None => writer.invalid("sequence option", "a value is required"),
        }
    }

    fn emit_required_type(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self.value.as_ref() {
            Some(SyntaxValue::Type(value)) => value.emit(writer),
            Some(SyntaxValue::Expression(_)) => {
                writer.invalid("AS sequence type", "a type name is required")
            }
            None => writer.invalid("AS sequence type", "a type name is required"),
        }
    }

    fn emit_owned_by(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let Some(SyntaxValue::Expression(value)) = self.value.as_ref() else {
            return writer.invalid("OWNED BY", "an ownership target is required");
        };
        value.emit_owned_by(writer)
    }
}

impl Expr {
    fn emit_owned_by(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::List(parts) => {
                if let [Self::Literal(Literal::String(name))] = parts.as_slice()
                    && name.eq_ignore_ascii_case("none")
                {
                    writer.sql("NONE");
                    return Ok(());
                }
                for (index, part) in parts.iter().enumerate() {
                    let Self::Literal(Literal::String(name)) = part else {
                        return writer.invalid("OWNED BY", "expected identifier components");
                    };
                    if index != 0 {
                        writer.sql(".");
                    }
                    writer.ident(name)?;
                }
                Ok(())
            }
            Self::Column(name) => name.emit(writer),
            Self::Literal(Literal::Keyword(keyword)) if keyword.eq_ignore_ascii_case("none") => {
                writer.sql("NONE");
                Ok(())
            }
            _ => writer.invalid("OWNED BY", "expected a qualified identifier or NONE"),
        }
    }
}

impl SqlEmit for DropTable {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.concurrent {
            return writer.invalid(
                "DROP TABLE CONCURRENTLY",
                "CONCURRENTLY is not legal for DROP TABLE",
            );
        }
        if self.names.is_empty() {
            return writer.invalid("DROP TABLE", "at least one table name is required");
        }
        writer.sql("DROP TABLE");
        if self.if_exists {
            writer.sql(" IF EXISTS");
        }
        writer.sql(" ");
        writer.list(&self.names, ", ")?;
        if self.cascade {
            writer.postgres_only("DROP TABLE CASCADE")?;
            writer.sql(" CASCADE");
        }
        Ok(())
    }
}

impl SqlEmit for Comment {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("COMMENT")?;
        let kind = self.object_kind.code();
        writer.sql("COMMENT ON ");
        Self::emit_comment_kind(writer, kind)?;
        writer.sql(" ");
        self.emit_comment_target(writer, kind)?;
        writer.sql(" IS ");
        if self.text.is_empty() {
            // PostgreSQL treats both NULL and an empty string as removing the comment.
            writer.sql("NULL");
        } else {
            writer.string(&self.text)?;
        }
        Ok(())
    }
}

impl Comment {
    fn emit_comment_kind(writer: &mut SqlWriter, kind: u32) -> Result<(), DeparseError> {
        let keyword = match kind {
            1 => "ACCESS METHOD",
            2 => "AGGREGATE",
            6 => "CAST",
            7 => "COLUMN",
            8 => "COLLATION",
            9 => "CONVERSION",
            10 => "DATABASE",
            13 => "DOMAIN",
            14 => "CONSTRAINT",
            15 => "EVENT TRIGGER",
            16 => "EXTENSION",
            17 => "FOREIGN DATA WRAPPER",
            18 => "SERVER",
            19 => "FOREIGN TABLE",
            20 => "FUNCTION",
            21 => "INDEX",
            22 => "LANGUAGE",
            23 => "LARGE OBJECT",
            24 => "MATERIALIZED VIEW",
            25 => "OPERATOR CLASS",
            26 => "OPERATOR",
            27 => "OPERATOR FAMILY",
            29 => "POLICY",
            30 => "PROCEDURE",
            31 => "PUBLICATION",
            34 => "ROLE",
            35 => "ROUTINE",
            36 => "RULE",
            37 => "SCHEMA",
            38 => "SEQUENCE",
            39 => "SUBSCRIPTION",
            40 => "STATISTICS",
            41 => "CONSTRAINT",
            42 => "TABLE",
            43 => "TABLESPACE",
            44 => "TRANSFORM",
            45 => "TRIGGER",
            46 => "TEXT SEARCH CONFIGURATION",
            47 => "TEXT SEARCH DICTIONARY",
            48 => "TEXT SEARCH PARSER",
            49 => "TEXT SEARCH TEMPLATE",
            50 => "TYPE",
            52 => "VIEW",
            _ => return writer.unsupported("COMMENT object kind"),
        };
        writer.sql(keyword);
        Ok(())
    }

    fn emit_comment_target(&self, writer: &mut SqlWriter, kind: u32) -> Result<(), DeparseError> {
        if kind == 6 {
            let Some(cast) = &self.target.cast else {
                return writer.invalid("COMMENT ON CAST", "source and target types are required");
            };
            writer.sql("(");
            cast.source.emit(writer)?;
            writer.sql(" AS ");
            cast.target.emit(writer)?;
            writer.sql(")");
            return Ok(());
        }
        if matches!(kind, 13 | 50) {
            let Some(type_name) = &self.target.type_name else {
                return writer.invalid("COMMENT target", "a type name is required");
            };
            return type_name.emit(writer);
        }
        if kind == 14 {
            let Some(target) = &self.target.domain_constraint else {
                return writer.invalid(
                    "COMMENT ON DOMAIN CONSTRAINT",
                    "a domain target is required",
                );
            };
            writer.ident(&target.constraint)?;
            writer.sql(" ON DOMAIN ");
            return target.domain.emit(writer);
        }
        if kind == 44 {
            let Some(target) = &self.target.transform else {
                return writer.invalid("COMMENT ON TRANSFORM", "a type and language are required");
            };
            writer.sql("FOR ");
            target.type_name.emit(writer)?;
            writer.sql(" LANGUAGE ");
            return writer.ident(&target.language);
        }
        if self.target.name.is_empty() {
            return writer.invalid("COMMENT target", "an object name is required");
        }
        match kind {
            7 => writer.names(&self.target.name, "."),
            29 | 36 | 41 | 45 => {
                if self.target.name.len() < 2 {
                    return writer
                        .invalid("COMMENT target", "the object requires its containing table");
                }
                let last = self.target.name.len() - 1;
                writer.ident(&self.target.name[last])?;
                writer.sql(" ON ");
                writer.names(&self.target.name[..last], ".")
            }
            25 | 27 => self.target.emit_using_target(writer),
            23 => {
                if self.target.name.len() != 1
                    || self.target.name[0].is_empty()
                    || !self.target.name[0]
                        .bytes()
                        .all(|byte| byte.is_ascii_digit())
                {
                    return writer.invalid(
                        "COMMENT ON LARGE OBJECT",
                        "expected a numeric large object identifier",
                    );
                }
                writer.sql(&self.target.name[0]);
                Ok(())
            }
            2 => self.target.emit_aggregate_target(writer),
            20 | 30 | 35 => self.target.emit_named_target(writer),
            26 => self.target.emit_operator_target(writer),
            _ => {
                if self.target.argument_types.is_some()
                    || self.target.cast.is_some()
                    || self.target.type_name.is_some()
                    || self.target.domain_constraint.is_some()
                    || self.target.transform.is_some()
                {
                    return writer.invalid(
                        "COMMENT target",
                        "this object kind does not accept an argument signature",
                    );
                }
                writer.names(&self.target.name, ".")
            }
        }
    }
}

impl SqlEmit for AlterOwner {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("ALTER OWNER")?;
        let Some(kind) = &self.object_kind else {
            return writer.invalid("ALTER OWNER", "an object kind is required");
        };
        let kind = kind.code();
        writer.sql("ALTER ");
        let keyword = match kind {
            2 => "AGGREGATE",
            8 => "COLLATION",
            9 => "CONVERSION",
            10 => "DATABASE",
            13 => "DOMAIN",
            15 => "EVENT TRIGGER",
            17 => "FOREIGN DATA WRAPPER",
            18 => "SERVER",
            20 => "FUNCTION",
            22 => "LANGUAGE",
            23 => "LARGE OBJECT",
            24 => "MATERIALIZED VIEW",
            25 => "OPERATOR CLASS",
            26 => "OPERATOR",
            27 => "OPERATOR FAMILY",
            30 => "PROCEDURE",
            31 => "PUBLICATION",
            35 => "ROUTINE",
            39 => "SUBSCRIPTION",
            40 => "STATISTICS",
            37 => "SCHEMA",
            38 => "SEQUENCE",
            42 => "TABLE",
            43 => "TABLESPACE",
            46 => "TEXT SEARCH CONFIGURATION",
            47 => "TEXT SEARCH DICTIONARY",
            50 => "TYPE",
            52 => "VIEW",
            _ => return writer.unsupported("ALTER OWNER object kind"),
        };
        writer.sql(keyword);
        writer.sql(" ");
        if let Some(relation) = &self.relation {
            if self.object.is_some() {
                return writer.invalid(
                    "ALTER OWNER target",
                    "a statement cannot provide both a relation and a named object",
                );
            }
            relation.emit(writer)?;
        } else {
            let Some(object) = &self.object else {
                return writer.invalid("ALTER OWNER target", "an object name is required");
            };
            if kind == 2 {
                object.emit_aggregate_target(writer)?;
            } else if matches!(kind, 20 | 30 | 35) {
                object.emit_named_target(writer)?;
            } else if kind == 26 {
                object.emit_operator_target(writer)?;
            } else if matches!(kind, 25 | 27) {
                object.emit_using_target(writer)?;
            } else if kind == 23 {
                object.emit_large_object_target(writer)?;
            } else if object.argument_types.is_some()
                || object.cast.is_some()
                || object.type_name.is_some()
                || object.domain_constraint.is_some()
                || object.transform.is_some()
            {
                return writer.invalid(
                    "ALTER OWNER target",
                    "this object kind does not accept an argument signature",
                );
            } else {
                writer.names(&object.name, ".")?;
            }
        }
        writer.sql(" OWNER TO ");
        self.owner.emit(writer)
    }
}

impl SqlEmit for ObjectArgument {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let Some(type_name) = &self.type_name else {
            if self.mode.is_some() || self.name.is_some() {
                return writer.invalid("operator argument", "NONE cannot have a mode or name");
            }
            writer.sql("NONE");
            return Ok(());
        };
        if let Some(mode) = &self.mode {
            match mode.as_str() {
                "IN" | "OUT" | "INOUT" | "VARIADIC" => {
                    writer.sql(mode);
                    writer.sql(" ");
                }
                _ => return writer.invalid("object argument mode", "unsupported signature mode"),
            }
        }
        if let Some(name) = &self.name {
            writer.ident(name)?;
            writer.sql(" ");
        }
        type_name.emit(writer)
    }
}

impl ObjectTarget {
    fn emit_aggregate_target(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.name.is_empty() {
            return writer.invalid("aggregate target", "an aggregate name is required");
        }
        writer.names(&self.name, ".")?;
        match self.argument_types.as_deref() {
            None | Some([]) => writer.sql("(*)"),
            Some(arguments) => {
                writer.sql("(");
                writer.list(arguments, ", ")?;
                writer.sql(")");
            }
        }
        Ok(())
    }

    fn emit_named_target(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.name.is_empty() {
            return writer.invalid("object target", "an object name is required");
        }
        writer.names(&self.name, ".")?;
        if let Some(argument_types) = &self.argument_types {
            writer.sql("(");
            writer.list(argument_types, ", ")?;
            writer.sql(")");
        }
        Ok(())
    }

    fn emit_operator_target(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.name.is_empty() {
            return writer.invalid("operator target", "an operator name is required");
        }
        Name::from_parts(&self.name).emit_operator(writer)?;
        let Some(argument_types) = &self.argument_types else {
            return writer.invalid("object signature", "an argument list is required");
        };
        if argument_types.len() != 2 {
            return writer.invalid(
                "operator signature",
                "exactly two argument types are required",
            );
        }
        writer.sql(" (");
        writer.list(argument_types, ", ")?;
        writer.sql(")");
        Ok(())
    }

    fn emit_large_object_target(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.name.len() != 1
            || self.name[0].is_empty()
            || !self.name[0].bytes().all(|byte| byte.is_ascii_digit())
        {
            return writer.invalid("ALTER LARGE OBJECT", "expected a numeric object identifier");
        }
        writer.sql(&self.name[0]);
        Ok(())
    }

    fn emit_using_target(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.name.len() < 2 {
            return writer.invalid(
                "operator class or family target",
                "an access method is required",
            );
        }
        writer.names(&self.name[1..], ".")?;
        writer.sql(" USING ");
        writer.ident(&self.name[0])
    }
}

impl Role {
    pub(in crate::ast) fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self.kind {
            1 => writer.ident(&self.name),
            2 => {
                writer.sql("CURRENT_ROLE");
                Ok(())
            }
            3 => {
                writer.sql("CURRENT_USER");
                Ok(())
            }
            4 => {
                writer.sql("SESSION_USER");
                Ok(())
            }
            5 => {
                writer.sql("PUBLIC");
                Ok(())
            }
            _ => writer.invalid("role", "unknown or missing role kind"),
        }
    }
}

#[cfg(test)]
mod object_signature_tests {
    use crate::ast::Dialect;

    fn assert_native_deparse_round_trip(sql: &str) {
        let statement = crate::parse_statements(sql, Dialect::postgres())
            .next()
            .expect("one statement")
            .expect(sql);
        let emitted = statement
            .deparse_sql(Dialect::postgres())
            .expect("statement emits");
        let expected = pg_query::parse(sql)
            .expect(sql)
            .deparse()
            .expect("native deparse");
        let actual = pg_query::parse(&emitted)
            .expect(&emitted)
            .deparse()
            .expect("emitted native deparse");
        assert_eq!(expected, actual, "emitted SQL: {emitted}");
    }

    #[test]
    fn comments_retain_function_and_operator_signatures() {
        assert_native_deparse_round_trip(
            "COMMENT ON FUNCTION app.f(IN x integer, INOUT y text) IS 'function comment'",
        );
        assert_native_deparse_round_trip(
            "COMMENT ON OPERATOR + (NONE, integer) IS 'operator comment'",
        );
    }

    #[test]
    fn alter_owner_retains_function_signature() {
        assert_native_deparse_round_trip(
            "ALTER FUNCTION app.f(IN integer, VARIADIC text[]) OWNER TO CURRENT_USER",
        );
    }
}

impl SqlEmit for View {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.replace && !writer.is_postgres() {
            return writer.invalid(
                "CREATE OR REPLACE VIEW",
                "SQLite does not support OR REPLACE VIEW",
            );
        }
        if self.if_not_exists && writer.is_postgres() && !self.materialized {
            return writer.invalid(
                "CREATE VIEW IF NOT EXISTS",
                "PostgreSQL does not support IF NOT EXISTS for views",
            );
        }
        if self.materialized && !writer.is_postgres() {
            return writer.invalid(
                "CREATE MATERIALIZED VIEW",
                "SQLite does not support materialized views",
            );
        }
        if self.temporary && self.materialized {
            return writer.invalid(
                "CREATE TEMP MATERIALIZED VIEW",
                "a materialized view cannot be temporary",
            );
        }
        if self.materialized && self.replace {
            return writer.invalid(
                "CREATE OR REPLACE MATERIALIZED VIEW",
                "PostgreSQL does not support replacing a materialized view",
            );
        }
        if self.materialized && !matches!(self.check_option, ViewCheckOption::None) {
            return writer.invalid(
                "MATERIALIZED VIEW CHECK OPTION",
                "check options apply only to ordinary views",
            );
        }
        if !writer.is_postgres()
            && (!self.options.is_empty() || !matches!(self.check_option, ViewCheckOption::None))
        {
            return writer.invalid(
                "VIEW options",
                "view options and check clauses require PostgreSQL",
            );
        }
        writer.sql("CREATE ");
        if self.replace {
            writer.sql("OR REPLACE ");
        }
        if self.temporary {
            writer.sql("TEMPORARY ");
        }
        if self.materialized {
            writer.sql("MATERIALIZED ");
        }
        writer.sql("VIEW");
        if self.if_not_exists {
            writer.sql(" IF NOT EXISTS");
        }
        writer.sql(" ");
        self.name.emit(writer)?;
        if !self.aliases.is_empty() {
            writer.sql(" (");
            writer.names(&self.aliases, ", ")?;
            writer.sql(")");
        }
        if !self.options.is_empty() {
            writer.sql(" WITH (");
            writer.list(&self.options, ", ")?;
            writer.sql(")");
        }
        writer.sql(" AS ");
        self.query.emit(writer)?;
        self.check_option.emit(writer)
    }
}

impl ViewCheckOption {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::None => Ok(()),
            Self::Local => {
                writer.postgres_only("WITH LOCAL CHECK OPTION")?;
                writer.sql(" WITH LOCAL CHECK OPTION");
                Ok(())
            }
            Self::Cascaded => {
                writer.postgres_only("WITH CASCADED CHECK OPTION")?;
                writer.sql(" WITH CASCADED CHECK OPTION");
                Ok(())
            }
        }
    }
}
