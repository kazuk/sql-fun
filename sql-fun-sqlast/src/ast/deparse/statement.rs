use super::{DeparseError, SqlEmit, SqlWriter};
use crate::ast::{
    PartitionBound, PartitionDatum, PartitionSpec, StatementNode, Table, TableExtras,
};

impl SqlEmit for StatementNode {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        crate::sem::with_stack(|| match self {
            Self::Query(v) => v.emit(writer),
            Self::Insert(v) => v.emit(writer),
            Self::Update(v) => v.emit(writer),
            Self::Delete(v) => v.emit(writer),
            Self::Merge(v) => v.emit(writer),
            Self::View(v) => v.emit(writer),
            Self::DropTrigger(_) => writer.unsupported("DROP TRIGGER"),
            Self::CreateTrigger(_) => writer.unsupported("CREATE TRIGGER"),
            Self::CreateRule(_) => writer.unsupported("CREATE RULE"),
            Self::ViewWrite(v) => v.emit(writer),
            Self::Extension(v) => v.emit(writer),
            Self::VariableSet(v) => v.emit(writer),
            Self::EnumType(v) => v.emit(writer),
            Self::Sequence(v) => v.emit(writer),
            Self::DropTable(v) => v.emit(writer),
            Self::AlterSequence(v) => v.emit(writer),
            Self::Comment(v) => v.emit(writer),
            Self::AlterOwner(v) => v.emit(writer),
            Self::CompositeType(v) => v.emit(writer),
            Self::Domain(v) => v.emit(writer),
            Self::Function(v) => v.emit(writer),
            Self::Schema(v) => v.emit(writer),
            Self::Index(v) => v.emit(writer),
            Self::TableAs(v) => v.emit(writer),
            Self::Table(v) => v.emit(writer),
            Self::AlterRelation(v) => v.emit(writer),
        })
        .map_err(|error| error.context(&format!("{:?}", self.kind())))
    }
}

impl SqlEmit for crate::sem::CreateTableMode {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Default => {}
            Self::IfNotExists => writer.sql("IF NOT EXISTS "),
            Self::OrReplace => {
                return writer.invalid(
                    "CREATE TABLE",
                    "OR REPLACE is not supported by the selected SQL dialect",
                );
            }
        }
        Ok(())
    }
}
impl SqlEmit for Table {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("CREATE ");
        if self.temporary {
            w.sql("TEMPORARY ");
        } else if self.extras.unlogged {
            w.postgres_only("UNLOGGED")?;
            w.sql("UNLOGGED ");
        }
        w.sql("TABLE ");
        self.mode.emit(w)?;
        self.name.emit(w)?;
        if let Some(query) = &self.query {
            w.sql(" AS ");
            query.emit(w)?;
            return Ok(());
        }
        if let Some(ty) = &self.extras.of_type {
            w.sql(" OF ");
            ty.emit(w)?;
        }
        if self.extras.partition_bound.is_some() {
            let [parent] = self.extras.inherits.as_slice() else {
                return w.invalid("PARTITION OF", "exactly one parent is required");
            };
            w.sql(" PARTITION OF ");
            parent.emit(w)?;
        }
        if !self.columns.is_empty()
            || !self.constraints.is_empty()
            || (self.extras.of_type.is_none() && self.extras.partition_bound.is_none())
        {
            w.sql(" (");
            w.list(&self.columns, ", ")?;
            if !self.columns.is_empty() && !self.constraints.is_empty() {
                w.sql(", ");
            }
            w.list(&self.constraints, ", ")?;
            w.sql(")");
        }
        self.extras.emit(w)?;
        if !self.options.is_empty() {
            if w.is_postgres() {
                w.sql(" WITH (");
                w.list(&self.options, ", ")?;
                w.sql(")");
            } else {
                for (i, option) in self.options.iter().enumerate() {
                    w.sql(if i == 0 { " " } else { ", " });
                    option.emit_sqlite_table_option(w)?;
                }
            }
        }
        if let Some(commit) = &self.extras.on_commit {
            commit.emit(w)?;
        }
        if let Some(tablespace) = &self.extras.tablespace {
            w.sql(" TABLESPACE ");
            w.ident(tablespace)?;
        }
        Ok(())
    }
}
impl SqlEmit for TableExtras {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if let Some(bound) = &self.partition_bound {
            bound.emit(w)?;
        } else if !self.inherits.is_empty() {
            w.sql(" INHERITS (");
            w.list(&self.inherits, ", ")?;
            w.sql(")");
        }
        if let Some(spec) = &self.partition_spec {
            spec.emit(w)?;
        }
        if let Some(method) = &self.access_method {
            w.sql(" USING ");
            w.ident(method)?;
        }
        Ok(())
    }
}
impl SqlEmit for PartitionSpec {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.postgres_only("PARTITION BY")?;
        if !matches!(self.strategy.as_str(), "LIST" | "RANGE" | "HASH") || self.keys.is_empty() {
            return w.invalid("PARTITION BY", "unknown strategy or missing keys");
        }
        w.sql(" PARTITION BY ");
        w.sql(&self.strategy);
        w.sql(" (");
        w.list(&self.keys, ", ")?;
        w.sql(")");
        Ok(())
    }
}
impl SqlEmit for PartitionBound {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.postgres_only("partition bound")?;
        if self.is_default {
            w.sql(" DEFAULT");
            return Ok(());
        }
        w.sql(" FOR VALUES ");
        match self.strategy.as_str() {
            "l" => self.emit_list_bound(w)?,
            "r" => self.emit_range_bound(w)?,
            "h" => self.emit_hash_bound(w),
            _ => return w.invalid("partition bound", "unknown strategy"),
        }
        Ok(())
    }
}

impl PartitionBound {
    fn emit_list_bound(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("IN (");
        w.list(&self.list, ", ")?;
        w.sql(")");
        Ok(())
    }

    fn emit_range_bound(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("FROM (");
        w.list(&self.lower, ", ")?;
        w.sql(") TO (");
        w.list(&self.upper, ", ")?;
        w.sql(")");
        Ok(())
    }

    fn emit_hash_bound(&self, w: &mut SqlWriter) {
        w.sql("WITH (MODULUS ");
        w.number(self.modulus);
        w.sql(", REMAINDER ");
        w.number(self.remainder);
        w.sql(")");
    }
}
impl SqlEmit for PartitionDatum {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Min => w.sql("MINVALUE"),
            Self::Max => w.sql("MAXVALUE"),
            Self::Value(v) => v.emit(w)?,
        };
        Ok(())
    }
}
impl crate::ast::SyntaxOption {
    fn emit_sqlite_table_option(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.value.is_some() || self.namespace.is_some() {
            return w.invalid("SQLite table option", "unexpected option payload");
        }
        match self.name.as_str() {
            "strict" => w.sql("STRICT"),
            "without_rowid" => w.sql("WITHOUT ROWID"),
            _ => return w.invalid("SQLite table option", "unknown option"),
        };
        Ok(())
    }
}

impl SqlEmit for crate::ast::view_write::WriteDeclaration {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.invalid(
            "view write metadata",
            "trigger/rule catalog metadata does not retain executable SQL for rendering",
        )
    }
}
