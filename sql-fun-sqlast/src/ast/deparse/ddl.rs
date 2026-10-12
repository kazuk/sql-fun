use crate::ast::ddl::{
    AlterColumnType, AlterConstraintFlags, AlterDefinition, AlterIdentity, AlterOperation,
    AlterPartition, CompositeField, CompositeType, Domain, Function, FunctionBody,
    FunctionBodyStatement, FunctionOption, FunctionParameter, GenericAction, GenericOption, Index,
    ParameterMode, PostgresAlterCommand, ReplicaIdentity, Schema, TableAs,
};
use crate::ast::deparse::{DeparseError, SqlEmit, SqlWriter};
use crate::ast::{AlterCommand, AlterRelation, AlterRelationKind};

impl SqlEmit for TableAs {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("CREATE TABLE AS")?;
        writer.sql(if self.materialized {
            "CREATE MATERIALIZED VIEW "
        } else {
            "CREATE "
        });
        if !self.materialized {
            match self.into.relation.persistence.as_str() {
                "t" => writer.sql("TEMPORARY "),
                "u" => writer.sql("UNLOGGED "),
                _ => (),
            }
            writer.sql("TABLE ");
        }
        if self.if_not_exists {
            writer.sql("IF NOT EXISTS ");
        }
        self.into.emit_without_persistence(writer)?;
        writer.sql(" AS ");
        self.query.emit(writer)?;
        if self.skip_data {
            writer.sql(" WITH NO DATA");
        }
        Ok(())
    }
}

impl SqlEmit for Index {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("CREATE INDEX")?;
        writer.sql("CREATE ");
        if self.unique {
            writer.sql("UNIQUE ");
        }
        writer.sql("INDEX ");
        if self.concurrent {
            writer.sql("CONCURRENTLY ");
        }
        if self.if_not_exists {
            writer.sql("IF NOT EXISTS ");
        }
        if !self.name.is_empty() {
            writer.ident(&self.name)?;
            writer.sql(" ");
        } else if self.if_not_exists {
            return writer.invalid("CREATE INDEX", "IF NOT EXISTS requires an index name");
        }
        writer.sql("ON ");
        self.relation.emit(writer)?;
        if !self.access_method.is_empty() {
            writer.sql(" USING ");
            writer.ident(&self.access_method)?;
        }
        if self.keys.is_empty() {
            return writer.invalid("CREATE INDEX", "index requires a key");
        }
        writer.sql(" (");
        writer.list(&self.keys, ", ")?;
        writer.sql(")");
        if !self.includes.is_empty() {
            writer.sql(" INCLUDE (");
            writer.list(&self.includes, ", ")?;
            writer.sql(")");
        }
        if self.nulls_not_distinct {
            writer.sql(" NULLS NOT DISTINCT");
        }
        if !self.options.is_empty() {
            writer.sql(" WITH (");
            writer.list(&self.options, ", ")?;
            writer.sql(")");
        }
        if !self.tablespace.is_empty() {
            writer.sql(" TABLESPACE ");
            writer.ident(&self.tablespace)?;
        }
        if let Some(predicate) = &self.predicate {
            writer.sql(" WHERE ");
            predicate.emit(writer)?;
        }
        Ok(())
    }
}

impl SqlEmit for Schema {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("CREATE SCHEMA")?;
        writer.sql("CREATE SCHEMA ");
        if self.if_not_exists {
            writer.sql("IF NOT EXISTS ");
        }
        if !self.name.is_empty() {
            writer.ident(&self.name)?;
        } else if self.authorization.is_none() {
            return writer.invalid("CREATE SCHEMA", "schema name or authorization is required");
        }
        if let Some(role) = &self.authorization {
            writer.sql(" AUTHORIZATION ");
            role.emit(writer)?;
        }
        if self.if_not_exists && !self.elements.is_empty() {
            return writer.invalid(
                "CREATE SCHEMA",
                "IF NOT EXISTS cannot include schema elements",
            );
        }
        for element in &self.elements {
            writer.sql(" ");
            element.emit(writer)?;
        }
        Ok(())
    }
}

impl SqlEmit for Function {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("CREATE FUNCTION")?;
        writer.sql("CREATE ");
        if self.replace {
            writer.sql("OR REPLACE ");
        }
        writer.sql(if self.procedure {
            "PROCEDURE "
        } else {
            "FUNCTION "
        });
        self.name.emit(writer)?;
        writer.sql("(");
        let mut separator = "";
        for parameter in &self.parameters {
            if !parameter.mode.is_table() {
                writer.sql(separator);
                parameter.emit(writer)?;
                separator = ", ";
            }
        }
        writer.sql(")");
        if self.has_table_parameters() {
            if self.procedure {
                return writer.invalid("CREATE PROCEDURE", "procedure cannot return a table");
            }
            writer.sql(" RETURNS TABLE (");
            let mut separator = "";
            for parameter in &self.parameters {
                if parameter.mode.is_table() {
                    writer.sql(separator);
                    parameter.emit(writer)?;
                    separator = ", ";
                }
            }
            writer.sql(")");
        } else if let Some(result_type) = &self.result_type {
            if self.procedure {
                return writer
                    .invalid("CREATE PROCEDURE", "procedure cannot have a RETURNS clause");
            }
            writer.sql(" RETURNS ");
            result_type.emit(writer)?;
        }
        for option in &self.options {
            writer.sql(" ");
            option.emit(writer)?;
        }
        if let Some(body) = &self.body {
            writer.sql(" ");
            body.emit(writer)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_ddl_families {
    use crate::ast::Dialect;
    use crate::parse_statements;

    #[rstest::rstest]
    #[case::string_body(
        "CREATE FUNCTION f(a integer DEFAULT 1) RETURNS integer LANGUAGE sql AS 'SELECT a' IMMUTABLE STRICT SECURITY DEFINER PARALLEL SAFE COST 5"
    )]
    #[case::c_body(
        "CREATE FUNCTION f(integer) RETURNS integer LANGUAGE c AS 'library', 'symbol' WINDOW LEAKPROOF SUPPORT support_fn"
    )]
    #[case::return_body(
        "CREATE OR REPLACE FUNCTION f(a integer) RETURNS integer LANGUAGE sql RETURN a + 1"
    )]
    #[case::atomic_body(
        "CREATE FUNCTION f(a integer) RETURNS integer LANGUAGE sql BEGIN ATOMIC SELECT a; RETURN a + 1; END"
    )]
    #[case::procedure(
        "CREATE PROCEDURE p(IN a integer, OUT b integer) LANGUAGE sql BEGIN ATOMIC SELECT a; END"
    )]
    #[case::returns_table(
        "CREATE FUNCTION f(a integer) RETURNS TABLE (b integer, c text) LANGUAGE sql AS 'SELECT a, a::text'"
    )]
    #[case::options(
        "CREATE FUNCTION f() RETURNS SETOF integer LANGUAGE sql AS 'SELECT 1' TRANSFORM FOR TYPE integer NOT LEAKPROOF CALLED ON NULL INPUT SECURITY INVOKER PARALLEL RESTRICTED ROWS 3 SET search_path TO app, public"
    )]
    #[case::index_options(
        "CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS idx ON ONLY t USING btree (a DESC NULLS LAST) INCLUDE (b) NULLS NOT DISTINCT WITH (fillfactor = 70) TABLESPACE space WHERE a > 0"
    )]
    #[case::unnamed_index("CREATE INDEX ON t (a)")]
    #[case::schema_auth(
        "CREATE SCHEMA app AUTHORIZATION CURRENT_USER CREATE TABLE t (a integer) CREATE VIEW v AS SELECT a FROM t"
    )]
    #[case::schema_role("CREATE SCHEMA AUTHORIZATION owner")]
    #[case::table_as(
        "CREATE TEMPORARY TABLE IF NOT EXISTS result (a) USING heap WITH (fillfactor = 70) ON COMMIT DROP AS SELECT 1 WITH NO DATA"
    )]
    #[case::materialized_view(
        "CREATE MATERIALIZED VIEW IF NOT EXISTS mv (a) USING heap TABLESPACE space AS SELECT 1 WITH NO DATA"
    )]
    #[case::composite(r#"CREATE TYPE pair AS (a numeric(8, 2), b text COLLATE "C", c integer[])"#)]
    #[case::domain(
        "CREATE DOMAIN positive AS numeric(8, 2) DEFAULT 1 NOT NULL CONSTRAINT positive_check CHECK (VALUE > 0)"
    )]
    fn retained_ddl_syntax_survives_rendering(#[case] sql: &str) {
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let emitted = statement.deparse_sql(Dialect::postgres()).unwrap();
        let reparsed = parse_statements(&emitted, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(
            emitted,
            reparsed.deparse_sql(Dialect::postgres()).unwrap(),
            "input: {sql}"
        );
    }

    #[test]
    fn structured_function_body_and_configuration_are_rendered() {
        let statement = parse_statements("CREATE FUNCTION f(a integer) RETURNS integer LANGUAGE sql SET search_path TO app BEGIN ATOMIC RETURN a + 1; END", Dialect::postgres()).next().unwrap().unwrap();
        let emitted = statement.deparse_sql(Dialect::postgres()).unwrap();
        assert!(emitted.contains("SET"), "{emitted}");
        assert!(emitted.contains("search_path"), "{emitted}");
        assert!(emitted.contains("BEGIN ATOMIC RETURN"), "{emitted}");
        assert!(emitted.contains(" + 1"), "{emitted}");
    }
}

impl Function {
    fn has_table_parameters(&self) -> bool {
        self.parameters.iter().any(FunctionParameter::is_table)
    }
}

impl FunctionParameter {
    const fn is_table(&self) -> bool {
        self.mode.is_table()
    }
}

impl SqlEmit for FunctionParameter {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.mode.emit(writer)?;
        if !self.name.is_empty() {
            writer.ident(&self.name)?;
            writer.sql(" ");
        }
        self.type_name.emit(writer)?;
        if let Some(default) = &self.default {
            writer.sql(" DEFAULT ");
            default.emit(writer)?;
        }
        Ok(())
    }
}

impl ParameterMode {
    const fn is_table(&self) -> bool {
        matches!(self, Self::Table)
    }
}

impl SqlEmit for ParameterMode {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql(match self {
            Self::In => "IN ",
            Self::Out => "OUT ",
            Self::InOut => "INOUT ",
            Self::Variadic => "VARIADIC ",
            Self::Table => "",
        });
        Ok(())
    }
}

impl SqlEmit for FunctionOption {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Body(parts) => Self::emit_body_strings(parts, writer)?,
            Self::Language(language) => {
                writer.sql("LANGUAGE ");
                writer.string(language)?;
            }
            Self::Transform(types) => Self::emit_transforms(types, writer)?,
            Self::Window => writer.sql("WINDOW"),
            Self::Volatility(value) => Self::emit_volatility(value, writer)?,
            Self::Leakproof(value) => {
                writer.sql(if *value { "LEAKPROOF" } else { "NOT LEAKPROOF" })
            }
            Self::Strict(value) => writer.sql(if *value {
                "RETURNS NULL ON NULL INPUT"
            } else {
                "CALLED ON NULL INPUT"
            }),
            Self::SecurityDefiner(value) => writer.sql(if *value {
                "SECURITY DEFINER"
            } else {
                "SECURITY INVOKER"
            }),
            Self::Parallel(value) => Self::emit_parallel(value, writer)?,
            Self::Cost(value) => {
                writer.sql("COST ");
                value.emit(writer)?;
            }
            Self::Rows(value) => {
                writer.sql("ROWS ");
                value.emit(writer)?;
            }
            Self::Support(name) => {
                writer.sql("SUPPORT ");
                name.emit(writer)?;
            }
            Self::Setting(setting) => setting.emit(writer)?,
        }
        Ok(())
    }
}

impl FunctionOption {
    fn emit_body_strings(parts: &[String], writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if parts.is_empty() || parts.len() > 2 {
            return writer.invalid("function AS", "function AS requires one or two strings");
        }
        writer.sql("AS ");
        for (index, part) in parts.iter().enumerate() {
            if index != 0 {
                writer.sql(", ");
            }
            writer.string(part)?;
        }
        Ok(())
    }

    fn emit_transforms(
        types: &[crate::ast::TypeName],
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        if types.is_empty() {
            return writer.invalid("function TRANSFORM", "TRANSFORM requires a type");
        }
        writer.sql("TRANSFORM ");
        for (index, type_name) in types.iter().enumerate() {
            if index != 0 {
                writer.sql(", ");
            }
            writer.sql("FOR TYPE ");
            type_name.emit(writer)?;
        }
        Ok(())
    }

    fn emit_volatility(value: &str, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if !["immutable", "stable", "volatile"].contains(&value) {
            return writer.invalid("function volatility", "unknown volatility");
        }
        writer.sql(&value.to_ascii_uppercase());
        Ok(())
    }

    fn emit_parallel(value: &str, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if !["safe", "unsafe", "restricted"].contains(&value) {
            return writer.invalid("function PARALLEL", "unknown parallel mode");
        }
        writer.sql("PARALLEL ");
        writer.sql(&value.to_ascii_uppercase());
        Ok(())
    }
}

impl SqlEmit for FunctionBody {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Return(expression) => {
                writer.sql("RETURN ");
                expression.emit(writer)
            }
            Self::Atomic(statements) => Self::emit_atomic(statements, writer),
        }
    }
}

impl FunctionBody {
    fn emit_atomic(
        statements: &[FunctionBodyStatement],
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.sql("BEGIN ATOMIC ");
        for statement in statements {
            statement.emit(writer)?;
            writer.sql("; ");
        }
        writer.sql("END");
        Ok(())
    }
}

impl SqlEmit for FunctionBodyStatement {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Return(expression) => {
                writer.sql("RETURN ");
                expression.emit(writer)
            }
            Self::Statement(statement) => statement.emit(writer),
        }
    }
}

impl SqlEmit for Domain {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("CREATE DOMAIN")?;
        writer.sql("CREATE DOMAIN ");
        self.name.emit(writer)?;
        writer.sql(" AS ");
        self.base.emit(writer)?;
        if !self.collation.is_empty() {
            writer.sql(" COLLATE ");
            self.collation.emit(writer)?;
        }
        for constraint in &self.constraints {
            writer.sql(" ");
            constraint.emit_as_column(writer)?;
        }
        Ok(())
    }
}

impl SqlEmit for CompositeType {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("CREATE TYPE AS")?;
        writer.sql("CREATE TYPE ");
        self.name.emit(writer)?;
        writer.sql(" AS (");
        writer.list(&self.fields, ", ")?;
        writer.sql(")");
        Ok(())
    }
}

impl SqlEmit for CompositeField {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.ident(&self.name)?;
        writer.sql(" ");
        self.type_name.emit(writer)?;
        if let Some(collation) = &self.collation {
            writer.sql(" COLLATE ");
            collation.emit(writer)?;
        }
        if self.is_not_null {
            return writer.invalid(
                "composite field",
                "composite attributes cannot have NOT NULL constraints",
            );
        }
        Ok(())
    }
}

impl SqlEmit for AlterRelation {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql("ALTER ");
        self.kind.emit(writer)?;
        writer.sql(" ");
        if self.if_exists {
            writer.sql("IF EXISTS ");
        }
        if self.only {
            writer.postgres_only("ALTER ONLY")?;
            if !self.kind.supports_only() {
                return writer.invalid("ALTER ONLY", "ONLY requires a table or foreign table");
            }
            writer.sql("ONLY ");
        }
        self.name.emit(writer)?;
        writer.sql(" ");
        if self.commands.is_empty() {
            return writer.invalid("ALTER relation", "ALTER requires a command");
        }
        if !writer.is_postgres() && self.commands.len() != 1 {
            return writer.invalid("ALTER TABLE", "SQLite ALTER TABLE requires one command");
        }
        let mut separator = "";
        for command in &self.commands {
            writer.sql(separator);
            command.emit_for_kind(self.kind, writer)?;
            separator = ", ";
        }
        Ok(())
    }
}

impl SqlEmit for AlterRelationKind {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql(match self {
            Self::Table => "TABLE",
            Self::ForeignTable => {
                writer.postgres_only("ALTER FOREIGN TABLE")?;
                "FOREIGN TABLE"
            }
            Self::Index => {
                writer.postgres_only("ALTER INDEX")?;
                "INDEX"
            }
            Self::CompositeType => {
                writer.postgres_only("ALTER TYPE")?;
                "TYPE"
            }
            Self::View => {
                writer.postgres_only("ALTER VIEW")?;
                "VIEW"
            }
            Self::MaterializedView => {
                writer.postgres_only("ALTER MATERIALIZED VIEW")?;
                "MATERIALIZED VIEW"
            }
            Self::Sequence => {
                writer.postgres_only("ALTER SEQUENCE")?;
                "SEQUENCE"
            }
        });
        Ok(())
    }
}

impl SqlEmit for AlterCommand {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Postgres(command) => command.emit(writer),
            Self::Owner(owner) => Self::emit_owner(owner, writer),
            Self::Default(column, expression) => {
                Self::emit_column_default(column, expression, writer)
            }
            Self::Constraint(constraint) => Self::emit_added_constraint(constraint, writer),
            Self::Cluster(index) => {
                writer.postgres_only("CLUSTER ON")?;
                writer.sql("CLUSTER ON ");
                writer.ident(index)
            }
            Self::Rename(name) => {
                writer.sql("RENAME TO ");
                name.emit(writer)
            }
            Self::AddColumn(column) => {
                writer.sql("ADD COLUMN ");
                column.emit(writer)
            }
            Self::AlterColumn(_, _) => writer.invalid(
                "ALTER COLUMN",
                "replacement column definitions are a Turso parser extension",
            ),
            Self::RenameColumn(from, to) => Self::emit_renamed_column(from, to, writer),
            Self::DropColumn(name) => {
                writer.sql("DROP COLUMN ");
                writer.ident(name)
            }
            Self::Unsupported(name) => writer.unsupported(name),
        }
    }
}

impl AlterCommand {
    fn emit_owner(owner: &str, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("OWNER TO")?;
        writer.sql("OWNER TO ");
        writer.ident(owner)
    }

    fn emit_column_default(
        column: &str,
        expression: &crate::ast::Expr,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.postgres_only("ALTER COLUMN DEFAULT")?;
        writer.sql("ALTER COLUMN ");
        writer.ident(column)?;
        writer.sql(" SET DEFAULT ");
        expression.emit(writer)
    }

    fn emit_added_constraint(
        constraint: &crate::ast::TableConstraint,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.postgres_only("ADD CONSTRAINT")?;
        writer.sql("ADD ");
        constraint.emit(writer)
    }

    fn emit_renamed_column(
        from: &str,
        to: &str,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.sql("RENAME COLUMN ");
        writer.ident(from)?;
        writer.sql(" TO ");
        writer.ident(to)
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_alter_command_emission {
    use crate::ast::Dialect;
    use crate::parse_statements;

    #[test]
    fn rejects_turso_column_replacement_for_stock_sqlite() {
        let statement =
            parse_statements("ALTER TABLE t ALTER COLUMN x TO x TEXT", Dialect::sqlite())
                .next()
                .unwrap()
                .unwrap();
        let error = statement.deparse_sql(Dialect::sqlite()).unwrap_err();
        assert!(error.is_invalid_syntax(), "{error}");
    }
}

impl AlterCommand {
    fn emit_for_kind(
        &self,
        kind: AlterRelationKind,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        match self {
            Self::Postgres(command) => command.emit_with_attribute(kind.uses_attribute(), writer),
            _ => self.emit(writer),
        }
    }
}

impl AlterRelationKind {
    const fn uses_attribute(self) -> bool {
        matches!(self, Self::CompositeType)
    }
}

impl SqlEmit for PostgresAlterCommand {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.emit_with_attribute(false, writer)
    }
}

impl PostgresAlterCommand {
    fn emit_with_attribute(
        &self,
        attribute: bool,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.postgres_only("PostgreSQL ALTER command")?;
        writer.sql(self.operation.prefix(attribute));
        if self.missing_ok && !self.operation.trailing_missing_ok() {
            writer.sql(if self.operation.is_add_column() {
                "IF NOT EXISTS "
            } else {
                "IF EXISTS "
            });
        }
        self.emit_target(writer)?;
        self.operation
            .emit_definition(&self.definition, self.missing_ok, writer)?;
        if self.cascade {
            writer.sql(" CASCADE");
        }
        Ok(())
    }

    fn emit_target(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if !self.name.is_empty() {
            writer.ident(&self.name)?;
            writer.sql(" ");
        } else if self.operation.is_access_method() {
            writer.sql("DEFAULT");
        }
        // PostgreSQL's num field names a column by ordinal. The statistics
        // target is retained separately in the definition, after SET STATISTICS.
        if self.number > 0 {
            writer.number(self.number);
            writer.sql(" ");
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_statistics_emission {
    use crate::ast::Dialect;
    use crate::parse_statements;

    #[rstest::rstest]
    #[case::named_numeric(
        "ALTER TABLE t ALTER COLUMN a SET STATISTICS 500",
        r#"ALTER TABLE "t" ALTER COLUMN "a" SET STATISTICS 500"#
    )]
    #[case::named_default(
        "ALTER TABLE t ALTER COLUMN a SET STATISTICS DEFAULT",
        r#"ALTER TABLE "t" ALTER COLUMN "a" SET STATISTICS DEFAULT"#
    )]
    #[case::ordinal_numeric(
        "ALTER TABLE t ALTER COLUMN 1 SET STATISTICS 500",
        r#"ALTER TABLE "t" ALTER COLUMN 1 SET STATISTICS 500"#
    )]
    #[case::ordinal_default(
        "ALTER TABLE t ALTER COLUMN 1 SET STATISTICS DEFAULT",
        r#"ALTER TABLE "t" ALTER COLUMN 1 SET STATISTICS DEFAULT"#
    )]
    #[case::zero(
        "ALTER TABLE t ALTER COLUMN a SET STATISTICS 0",
        r#"ALTER TABLE "t" ALTER COLUMN "a" SET STATISTICS 0"#
    )]
    #[case::negative(
        "ALTER TABLE t ALTER COLUMN a SET STATISTICS -1",
        r#"ALTER TABLE "t" ALTER COLUMN "a" SET STATISTICS -1"#
    )]
    fn keeps_column_and_statistics_target_distinct(#[case] sql: &str, #[case] expected: &str) {
        let original = pg_query::parse(sql).unwrap().deparse().unwrap();
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let emitted = statement.deparse_sql(Dialect::postgres()).unwrap();
        assert_eq!(expected, emitted);
        let reparsed = pg_query::parse(&emitted).unwrap().deparse().unwrap();
        assert_eq!(original, reparsed, "emitted SQL: {emitted}");
    }
}

#[cfg(test)]
mod test_alter_emission {
    use crate::ast::Dialect;
    use crate::parse_statements;

    #[rstest::rstest]
    #[case::add_column("ALTER TABLE ONLY t ADD COLUMN IF NOT EXISTS a integer DEFAULT 2")]
    #[case::foreign_options(
        "ALTER FOREIGN TABLE t OPTIONS (ADD host 'server', SET port '5432', DROP old)"
    )]
    #[case::foreign_only("ALTER FOREIGN TABLE ONLY t ALTER COLUMN a SET NOT NULL")]
    #[case::foreign_column_options(
        "ALTER FOREIGN TABLE t ALTER COLUMN a OPTIONS (SET column_name 'remote_a')"
    )]
    #[case::index_options("ALTER INDEX idx SET (fillfactor = 70), SET TABLESPACE space")]
    #[case::view_default("ALTER VIEW v ALTER COLUMN a SET DEFAULT 1")]
    #[case::materialized_options("ALTER MATERIALIZED VIEW mv SET (fillfactor = 70)")]
    #[case::sequence_owner("ALTER SEQUENCE seq OWNER TO CURRENT_USER")]
    #[case::type_attributes(
        "ALTER TYPE pair ADD ATTRIBUTE c integer, ALTER ATTRIBUTE a TYPE numeric(8, 2) CASCADE, DROP ATTRIBUTE b RESTRICT"
    )]
    #[case::drop_default("ALTER TABLE t ALTER COLUMN a DROP DEFAULT")]
    #[case::nullability("ALTER TABLE t ALTER COLUMN a SET NOT NULL, ALTER COLUMN b DROP NOT NULL")]
    #[case::statistics("ALTER TABLE t ALTER COLUMN a SET STATISTICS 500")]
    #[case::storage(
        "ALTER TABLE t ALTER COLUMN a SET STORAGE external, ALTER COLUMN a SET COMPRESSION default"
    )]
    #[case::column_options(
        "ALTER TABLE t ALTER COLUMN a SET (n_distinct = 10), ALTER COLUMN b RESET (n_distinct)"
    )]
    #[case::drop_column("ALTER TABLE t DROP COLUMN IF EXISTS a CASCADE")]
    #[case::constraint("ALTER TABLE t ADD CONSTRAINT positive CHECK (a > 0) NOT VALID")]
    #[case::constraint_flags("ALTER TABLE t ALTER CONSTRAINT fk DEFERRABLE INITIALLY DEFERRED")]
    #[case::constraint_validation(
        "ALTER TABLE t VALIDATE CONSTRAINT c, DROP CONSTRAINT IF EXISTS d CASCADE"
    )]
    #[case::column_type("ALTER TABLE t ALTER COLUMN a TYPE numeric(8, 2) USING a::numeric")]
    #[case::collated_type(r#"ALTER TABLE t ALTER COLUMN a TYPE text COLLATE "C" USING a::text"#)]
    #[case::owner("ALTER TABLE t OWNER TO CURRENT_USER")]
    #[case::cluster("ALTER TABLE t CLUSTER ON idx, SET WITHOUT CLUSTER")]
    #[case::logging("ALTER TABLE t SET UNLOGGED")]
    #[case::access_method("ALTER TABLE t SET ACCESS METHOD heap")]
    #[case::tablespace("ALTER TABLE t SET TABLESPACE space")]
    #[case::relation_options("ALTER TABLE t SET (fillfactor = 70), RESET (autovacuum_enabled)")]
    #[case::trigger("ALTER TABLE t ENABLE ALWAYS TRIGGER trg, DISABLE TRIGGER ALL")]
    #[case::rule("ALTER TABLE t ENABLE REPLICA RULE rule_name")]
    #[case::inheritance("ALTER TABLE t INHERIT parent, NO INHERIT former")]
    #[case::typed_table("ALTER TABLE t OF pair")]
    #[case::replica_identity("ALTER TABLE t REPLICA IDENTITY USING INDEX idx")]
    #[case::row_security("ALTER TABLE t ENABLE ROW LEVEL SECURITY, FORCE ROW LEVEL SECURITY")]
    #[case::attach_partition("ALTER TABLE t ATTACH PARTITION child FOR VALUES FROM (1) TO (10)")]
    #[case::detach_partition("ALTER TABLE t DETACH PARTITION child CONCURRENTLY")]
    #[case::detach_finalize("ALTER TABLE t DETACH PARTITION child FINALIZE")]
    #[case::add_identity(
        "ALTER TABLE t ALTER COLUMN a ADD GENERATED ALWAYS AS IDENTITY (START WITH 10 INCREMENT BY 2)"
    )]
    #[case::set_identity(
        "ALTER TABLE t ALTER COLUMN a SET GENERATED BY DEFAULT SET INCREMENT BY 2"
    )]
    #[case::restart_identity("ALTER TABLE t ALTER COLUMN a RESTART")]
    #[case::restart_identity_value("ALTER TABLE t ALTER COLUMN a RESTART WITH 10")]
    #[case::restart_identity_combined(
        "ALTER TABLE t ALTER COLUMN a SET GENERATED ALWAYS RESTART WITH 10 SET INCREMENT BY 2"
    )]
    #[case::drop_identity("ALTER TABLE t ALTER COLUMN a DROP IDENTITY IF EXISTS")]
    #[case::set_expression("ALTER TABLE t ALTER COLUMN a SET EXPRESSION AS (b + 1)")]
    #[case::drop_expression("ALTER TABLE t ALTER COLUMN a DROP EXPRESSION IF EXISTS")]
    fn postgres_alter_actions_preserve_native_structure(#[case] sql: &str) {
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let emitted = statement.deparse_sql(Dialect::postgres()).unwrap();
        let original = pg_query::parse(sql).unwrap().deparse().unwrap();
        let reparsed = pg_query::parse(&emitted).unwrap().deparse().unwrap();
        assert_eq!(original, reparsed, "emitted SQL: {emitted}");
    }
}

impl AlterOperation {
    const fn is_access_method(&self) -> bool {
        matches!(self, Self::SetAccessMethod)
    }
    const fn is_add_column(&self) -> bool {
        matches!(self, Self::AddColumn)
    }
    const fn trailing_missing_ok(&self) -> bool {
        matches!(self, Self::DropExpression | Self::DropIdentity)
    }
    const fn prefix(&self, attribute: bool) -> &'static str {
        if attribute {
            match self {
                Self::AddColumn => return "ADD ATTRIBUTE ",
                Self::AlterColumnType => return "ALTER ATTRIBUTE ",
                Self::DropColumn => return "DROP ATTRIBUTE ",
                _ => (),
            }
        }
        match self {
            Self::AddColumn => "ADD COLUMN ",
            Self::ColumnDefault
            | Self::DropNotNull
            | Self::SetNotNull
            | Self::SetExpression
            | Self::DropExpression
            | Self::SetStatistics
            | Self::SetOptions
            | Self::ResetOptions
            | Self::SetStorage
            | Self::SetCompression
            | Self::AlterColumnType
            | Self::AlterColumnGenericOptions
            | Self::AddIdentity
            | Self::SetIdentity
            | Self::DropIdentity => "ALTER COLUMN ",
            Self::DropColumn => "DROP COLUMN ",
            Self::AddConstraint => "ADD ",
            Self::AlterConstraint => "ALTER CONSTRAINT ",
            Self::ValidateConstraint => "VALIDATE CONSTRAINT ",
            Self::DropConstraint => "DROP CONSTRAINT ",
            Self::Owner => "OWNER TO ",
            Self::Cluster => "CLUSTER ON ",
            Self::DropCluster => "SET WITHOUT CLUSTER",
            Self::SetLogged => "SET LOGGED",
            Self::SetUnlogged => "SET UNLOGGED",
            Self::DropOids => "SET WITHOUT OIDS",
            Self::SetAccessMethod => "SET ACCESS METHOD ",
            Self::SetTablespace => "SET TABLESPACE ",
            Self::SetRelOptions => "SET ",
            Self::ResetRelOptions => "RESET ",
            Self::EnableTrigger => "ENABLE TRIGGER ",
            Self::EnableAlwaysTrigger => "ENABLE ALWAYS TRIGGER ",
            Self::EnableReplicaTrigger => "ENABLE REPLICA TRIGGER ",
            Self::DisableTrigger => "DISABLE TRIGGER ",
            Self::EnableTriggerAll => "ENABLE TRIGGER ALL",
            Self::DisableTriggerAll => "DISABLE TRIGGER ALL",
            Self::EnableTriggerUser => "ENABLE TRIGGER USER",
            Self::DisableTriggerUser => "DISABLE TRIGGER USER",
            Self::EnableRule => "ENABLE RULE ",
            Self::EnableAlwaysRule => "ENABLE ALWAYS RULE ",
            Self::EnableReplicaRule => "ENABLE REPLICA RULE ",
            Self::DisableRule => "DISABLE RULE ",
            Self::AddInherit => "INHERIT ",
            Self::DropInherit => "NO INHERIT ",
            Self::AddOf => "OF ",
            Self::DropOf => "NOT OF",
            Self::ReplicaIdentity => "REPLICA IDENTITY ",
            Self::EnableRowSecurity => "ENABLE ROW LEVEL SECURITY",
            Self::DisableRowSecurity => "DISABLE ROW LEVEL SECURITY",
            Self::ForceRowSecurity => "FORCE ROW LEVEL SECURITY",
            Self::NoForceRowSecurity => "NO FORCE ROW LEVEL SECURITY",
            Self::GenericOptions => "",
            Self::AttachPartition => "ATTACH PARTITION ",
            Self::DetachPartition | Self::DetachPartitionFinalize => "DETACH PARTITION ",
        }
    }

    fn emit_definition(
        &self,
        definition: &AlterDefinition,
        missing_ok: bool,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        match self {
            Self::ColumnDefault => definition.emit_column_default(writer),
            Self::SetStatistics => definition.emit_statistics(writer),
            Self::SetExpression => definition.emit_set_expression(writer),
            Self::SetIdentity => definition.emit_identity_options(writer),
            Self::SetAccessMethod if definition.is_none() => Ok(()),
            Self::DetachPartitionFinalize => definition.emit_finalized_partition(writer),
            _ => self.emit_standard_definition(definition, missing_ok, writer),
        }
    }

    fn emit_standard_definition(
        &self,
        definition: &AlterDefinition,
        missing_ok: bool,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.sql(self.definition_prefix());
        if missing_ok && self.trailing_missing_ok() {
            writer.sql(" IF EXISTS");
        }
        definition.emit(writer)
    }

    const fn definition_prefix(&self) -> &'static str {
        match self {
            Self::DropNotNull => "DROP NOT NULL",
            Self::SetNotNull => "SET NOT NULL",
            Self::DropExpression => "DROP EXPRESSION",
            Self::DropIdentity => "DROP IDENTITY",
            Self::SetOptions => "SET ",
            Self::ResetOptions => "RESET ",
            Self::SetStorage => "SET STORAGE ",
            Self::SetCompression => "SET COMPRESSION ",
            Self::AlterColumnType => "TYPE ",
            Self::AddIdentity => "ADD ",
            _ => "",
        }
    }
}

impl AlterDefinition {
    const fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
    fn emit_column_default(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.is_none() {
            writer.sql("DROP DEFAULT");
            Ok(())
        } else {
            writer.sql("SET DEFAULT ");
            self.emit(writer)
        }
    }

    fn emit_statistics(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql("SET STATISTICS ");
        if self.is_none() {
            writer.sql("DEFAULT");
            Ok(())
        } else {
            self.emit(writer)
        }
    }

    fn emit_set_expression(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql("SET EXPRESSION AS (");
        self.emit(writer)?;
        writer.sql(")");
        Ok(())
    }

    fn emit_finalized_partition(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.emit(writer)?;
        writer.sql(" FINALIZE");
        Ok(())
    }

    fn emit_identity_options(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Options(options) => Self::emit_identity_option_list(options, writer),
            _ => writer.invalid("ALTER IDENTITY", "identity options are missing"),
        }
    }

    fn emit_identity_option_list(
        options: &[crate::ast::SyntaxOption],
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        for (index, option) in options.iter().enumerate() {
            if index != 0 {
                writer.sql(" ");
            }
            option.emit_alter_identity_option(writer)?;
        }
        Ok(())
    }
}

impl SqlEmit for AlterDefinition {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::None => Ok(()),
            Self::Expression(value) => value.emit(writer),
            Self::Text(value) => writer.ident(value),
            Self::Options(options) => Self::emit_options(options, writer),
            Self::GenericOptions(options) => Self::emit_generic_options(options, writer),
            Self::Column(value) => value.emit(writer),
            Self::ColumnType(value) => value.emit(writer),
            Self::Constraint(value) => value.emit(writer),
            Self::ConstraintFlags(value) => value.emit(writer),
            Self::Role(value) => value.emit(writer),
            Self::Relation(value) => value.emit(writer),
            Self::Type(value) => value.emit(writer),
            Self::Replica(value) => value.emit(writer),
            Self::Partition(value) => value.emit(writer),
            Self::Identity(value) => value.emit(writer),
        }
    }
}

impl AlterDefinition {
    fn emit_options(
        options: &[crate::ast::SyntaxOption],
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.sql("(");
        writer.list(options, ", ")?;
        writer.sql(")");
        Ok(())
    }

    fn emit_generic_options(
        options: &[GenericOption],
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.sql("OPTIONS (");
        writer.list(options, ", ")?;
        writer.sql(")");
        Ok(())
    }
}

impl SqlEmit for AlterColumnType {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.type_name.emit(writer)?;
        if let Some(collation) = &self.collation {
            writer.sql(" COLLATE ");
            collation.emit(writer)?;
        }
        if let Some(using) = &self.using {
            writer.sql(" USING ");
            using.emit(writer)?;
        }
        Ok(())
    }
}

impl SqlEmit for AlterConstraintFlags {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.ident(&self.name)?;
        writer.sql(if self.deferrable {
            " DEFERRABLE"
        } else {
            " NOT DEFERRABLE"
        });
        writer.sql(if self.initially_deferred {
            " INITIALLY DEFERRED"
        } else {
            " INITIALLY IMMEDIATE"
        });
        Ok(())
    }
}

impl SqlEmit for GenericOption {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.action.emit(writer)?;
        writer.ident(&self.name)?;
        if let Some(value) = &self.value {
            writer.sql(" ");
            writer.string(value)?;
        }
        Ok(())
    }
}

impl SqlEmit for GenericAction {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql(match self {
            Self::Unspecified => "",
            Self::Add => "ADD ",
            Self::Set => "SET ",
            Self::Drop => "DROP ",
        });
        Ok(())
    }
}

impl SqlEmit for ReplicaIdentity {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self.kind.as_str() {
            "d" => writer.sql("DEFAULT"),
            "n" => writer.sql("NOTHING"),
            "f" => writer.sql("FULL"),
            "i" => {
                writer.sql("USING INDEX ");
                writer.ident(&self.index)?;
            }
            _ => return writer.invalid("REPLICA IDENTITY", "unknown replica identity mode"),
        }
        Ok(())
    }
}

impl SqlEmit for AlterPartition {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.relation.emit(writer)?;
        if let Some(bound) = &self.bound {
            bound.emit(writer)?;
        }
        if self.concurrent {
            writer.sql(" CONCURRENTLY");
        }
        Ok(())
    }
}

impl SqlEmit for AlterIdentity {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql(match self.mode.as_str() {
            "a" => "GENERATED ALWAYS AS IDENTITY",
            "d" => "GENERATED BY DEFAULT AS IDENTITY",
            _ => return writer.invalid("IDENTITY", "unknown identity generation mode"),
        });
        if !self.options.is_empty() {
            writer.sql(" (");
            for (index, option) in self.options.iter().enumerate() {
                if index != 0 {
                    writer.sql(" ");
                }
                option.emit_sequence_option(writer)?;
            }
            writer.sql(")");
        }
        Ok(())
    }
}

impl crate::ast::SyntaxOption {
    fn emit_alter_identity_option(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        // RESTART is an identity action in its own right. The other identity
        // options use SET, matching alter_identity_column_option grammar.
        if self.name == "restart" {
            return self.emit_sequence_option(writer);
        }
        writer.sql("SET ");
        if self.name == "generated" {
            writer.sql("GENERATED ");
            let Some(value) = &self.value else {
                return writer.invalid("IDENTITY", "generation mode is missing");
            };
            value.emit_identity_mode(writer)
        } else {
            self.emit_sequence_option(writer)
        }
    }
}

impl crate::ast::SyntaxValue {
    fn emit_identity_mode(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Expression(value) => value.emit_identity_mode(writer),
            _ => writer.invalid("IDENTITY", "generation mode must be a retained literal"),
        }
    }
}

impl crate::ast::Expr {
    fn emit_identity_mode(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Literal(value) => value.emit_identity_mode(writer),
            _ => writer.invalid("IDENTITY", "generation mode must be a string"),
        }
    }
}

impl crate::ast::Literal {
    fn emit_identity_mode(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::String(value) if value == "a" => writer.sql("ALWAYS"),
            Self::String(value) if value == "d" => writer.sql("BY DEFAULT"),
            Self::Integer(value) if *value == i32::from(b'a') => writer.sql("ALWAYS"),
            Self::Integer(value) if *value == i32::from(b'd') => writer.sql("BY DEFAULT"),
            _ => return writer.invalid("IDENTITY", "unknown generation mode"),
        };
        Ok(())
    }
}
