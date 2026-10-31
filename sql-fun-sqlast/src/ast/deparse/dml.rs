use super::{SqlEmit, SqlWriter};
use crate::ast::{
    Assignment, Conflict, ConflictAction, Delete, Expr, Insert, OverridingKind, Relation, Update,
};

impl SqlEmit for Insert {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), super::DeparseError> {
        self.with.emit(writer)?;
        writer.sql("INSERT");
        if let Some(conflict) = &self.or_conflict {
            if writer.is_postgres() {
                return writer.invalid(
                    "INSERT OR",
                    "SQLite conflict handling is not legal PostgreSQL SQL",
                );
            }
            writer.sql(" OR ");
            conflict.emit(writer)?;
        }
        writer.sql(" INTO ");
        self.relation.emit_insert_target(writer)?;
        if !self.columns.is_empty() {
            writer.sql(" (");
            for (index, column) in self.columns.iter().enumerate() {
                if index != 0 {
                    writer.sql(", ");
                }
                if column.value.is_some() {
                    return writer.invalid(
                        "INSERT column",
                        "a target column cannot have an assignment value",
                    );
                }
                column.emit_target(writer)?;
            }
            writer.sql(")");
        }
        if let Some(overriding) = &self.overriding {
            overriding.emit(writer)?;
        }
        if let Some(source) = &self.source {
            writer.sql(" ");
            source.emit(writer)?;
        } else {
            writer.sql(" DEFAULT VALUES");
        }
        if let Some(conflict) = &self.conflict {
            writer.sql(" ");
            conflict.emit(writer)?;
        }
        if !self.returning.is_empty() {
            writer.sql(" RETURNING ");
            crate::ast::ReturningOption::emit_options(&self.returning_options, writer)?;
            writer.list(&self.returning, ", ")?;
        }
        Ok(())
    }
}

impl SqlEmit for Update {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), super::DeparseError> {
        self.with.emit(writer)?;
        writer.sql("UPDATE");
        if let Some(conflict) = &self.or_conflict {
            if writer.is_postgres() {
                return writer.invalid(
                    "UPDATE OR",
                    "SQLite conflict handling is not legal PostgreSQL SQL",
                );
            }
            writer.sql(" OR ");
            conflict.emit(writer)?;
        }
        writer.sql(" ");
        self.relation.emit(writer)?;
        if self.assignments.is_empty() {
            return writer.invalid("UPDATE SET", "an UPDATE requires at least one assignment");
        }
        writer.sql(" SET ");
        Assignment::emit_list(writer, &self.assignments)?;
        if !self.from.is_empty() {
            writer.sql(" FROM ");
            writer.list(&self.from, ", ")?;
        }
        if let Some(predicate) = &self.predicate {
            writer.sql(" WHERE ");
            predicate.emit(writer)?;
        }
        if !self.returning.is_empty() {
            writer.sql(" RETURNING ");
            crate::ast::ReturningOption::emit_options(&self.returning_options, writer)?;
            writer.list(&self.returning, ", ")?;
        }
        Ok(())
    }
}

impl SqlEmit for Delete {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), super::DeparseError> {
        self.with.emit(writer)?;
        writer.sql("DELETE FROM ");
        self.relation.emit(writer)?;
        if !self.using.is_empty() {
            writer.postgres_only("DELETE USING")?;
            writer.sql(" USING ");
            writer.list(&self.using, ", ")?;
        }
        if let Some(predicate) = &self.predicate {
            writer.sql(" WHERE ");
            predicate.emit(writer)?;
        }
        if !self.returning.is_empty() {
            writer.sql(" RETURNING ");
            crate::ast::ReturningOption::emit_options(&self.returning_options, writer)?;
            writer.list(&self.returning, ", ")?;
        }
        Ok(())
    }
}

impl Assignment {
    pub(in crate::ast) fn emit_target(
        &self,
        writer: &mut SqlWriter,
    ) -> Result<(), super::DeparseError> {
        writer.ident(&self.name)?;
        for access in &self.indirection {
            access.emit(writer)?;
        }
        Ok(())
    }

    fn emit(&self, writer: &mut SqlWriter) -> Result<(), super::DeparseError> {
        self.emit_target(writer)?;
        writer.sql(" = ");
        match &self.value {
            Some(value) => value.emit(writer),
            None => writer.invalid("assignment", "an UPDATE assignment is missing its value"),
        }
    }

    pub(in crate::ast) fn emit_list(
        writer: &mut SqlWriter,
        assignments: &[Self],
    ) -> Result<(), super::DeparseError> {
        let mut index = 0;
        while index < assignments.len() {
            if index != 0 {
                writer.sql(", ");
            }
            let assignment = &assignments[index];
            let Some(value) = &assignment.value else {
                return writer.invalid("assignment", "an UPDATE assignment is missing its value");
            };
            if let Some((source, column, count)) = value.multi_assignment_data() {
                if column != 1 || count <= 0 {
                    return writer.invalid(
                        "tuple assignment",
                        "tuple assignment columns must begin at one and have a positive width",
                    );
                }
                let count = count as usize;
                let Some(end) = index.checked_add(count) else {
                    return writer.invalid(
                        "tuple assignment",
                        "tuple assignment width exceeds the target list",
                    );
                };
                let Some(group) = assignments.get(index..end) else {
                    return writer.invalid(
                        "tuple assignment",
                        "tuple assignment is missing one or more target columns",
                    );
                };
                let expected_source = Self::render_source(source, writer)?;
                for (offset, member) in group.iter().enumerate() {
                    let Some(member_value) = &member.value else {
                        return writer.invalid(
                            "tuple assignment",
                            "tuple assignment is missing a source expression",
                        );
                    };
                    let Some((member_source, member_column, member_count)) =
                        member_value.multi_assignment_data()
                    else {
                        return writer.invalid(
                            "tuple assignment",
                            "tuple assignment targets must form one complete group",
                        );
                    };
                    if member_column != (offset + 1) as i32 || member_count != count as i32 {
                        return writer.invalid(
                            "tuple assignment",
                            "tuple assignment columns must be consecutive and have matching widths",
                        );
                    }
                    if Self::render_source(member_source, writer)? != expected_source {
                        return writer.invalid(
                            "tuple assignment",
                            "tuple assignment targets must share one source expression",
                        );
                    }
                }
                writer.sql("(");
                for (offset, member) in group.iter().enumerate() {
                    if offset != 0 {
                        writer.sql(", ");
                    }
                    member.emit_target(writer)?;
                }
                writer.sql(") = ");
                source.emit(writer)?;
                index = end;
            } else {
                assignment.emit(writer)?;
                index += 1;
            }
        }
        Ok(())
    }

    fn render_source(source: &Expr, writer: &SqlWriter) -> Result<String, super::DeparseError> {
        let mut scratch = SqlWriter::new(writer.dialect());
        source.emit(&mut scratch)?;
        Ok(scratch.sql)
    }
}

impl SqlEmit for Conflict {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), super::DeparseError> {
        if self.next.is_some() && writer.is_postgres() {
            return writer.invalid(
                "ON CONFLICT",
                "chained conflict clauses are not legal PostgreSQL SQL",
            );
        }
        if self.constraint.is_some() {
            writer.postgres_only("ON CONSTRAINT")?;
            if !self.inference.is_empty() || self.inference_predicate.is_some() {
                return writer.invalid(
                    "ON CONFLICT target",
                    "a constraint target cannot be combined with an index inference target",
                );
            }
        }
        writer.sql("ON CONFLICT");
        if let Some(constraint) = &self.constraint {
            writer.sql(" ON CONSTRAINT ");
            writer.ident(constraint)?;
        } else if !self.inference.is_empty() {
            writer.sql(" (");
            writer.list(&self.inference, ", ")?;
            writer.sql(")");
            if let Some(predicate) = &self.inference_predicate {
                writer.sql(" WHERE ");
                predicate.emit(writer)?;
            }
        } else if self.inference_predicate.is_some() {
            return writer.invalid(
                "ON CONFLICT WHERE",
                "an index predicate requires an inference target",
            );
        }
        let Some(action) = self.action.as_ref() else {
            return writer.invalid("ON CONFLICT action", "a conflict action is required");
        };
        if writer.is_postgres()
            && matches!(action, ConflictAction::Update)
            && self.constraint.is_none()
            && self.inference.is_empty()
        {
            return writer.invalid(
                "ON CONFLICT DO UPDATE",
                "PostgreSQL DO UPDATE requires an inference or constraint target",
            );
        }
        action.emit(writer)?;
        if matches!(action, ConflictAction::Update) {
            if self.assignments.is_empty() {
                return writer.invalid(
                    "ON CONFLICT DO UPDATE SET",
                    "an update action requires at least one assignment",
                );
            }
            writer.sql(" ");
            Assignment::emit_list(writer, &self.assignments)?;
            if let Some(predicate) = &self.predicate {
                writer.sql(" WHERE ");
                predicate.emit(writer)?;
            }
        } else if !self.assignments.is_empty() || self.predicate.is_some() {
            return writer.invalid(
                "ON CONFLICT DO NOTHING",
                "DO NOTHING cannot have assignments or an update predicate",
            );
        }
        if let Some(next) = &self.next {
            writer.sql(" ");
            next.emit(writer)?;
        }
        Ok(())
    }
}

impl ConflictAction {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), super::DeparseError> {
        match self {
            Self::None => writer.invalid("ON CONFLICT action", "a conflict action is required"),
            Self::Nothing => {
                writer.sql(" DO NOTHING");
                Ok(())
            }
            Self::Update => {
                writer.sql(" DO UPDATE SET");
                Ok(())
            }
        }
    }
}

impl OverridingKind {
    pub(in crate::ast) fn emit(&self, writer: &mut SqlWriter) -> Result<(), super::DeparseError> {
        match self {
            Self::NotSet => Ok(()),
            Self::UserValue => {
                writer.postgres_only("OVERRIDING USER VALUE")?;
                writer.sql(" OVERRIDING USER VALUE");
                Ok(())
            }
            Self::SystemValue => {
                writer.postgres_only("OVERRIDING SYSTEM VALUE")?;
                writer.sql(" OVERRIDING SYSTEM VALUE");
                Ok(())
            }
        }
    }
}

impl Relation {
    fn emit_insert_target(&self, writer: &mut SqlWriter) -> Result<(), super::DeparseError> {
        if writer.is_postgres() && !self.inherit {
            return writer.invalid("INSERT target", "PostgreSQL INSERT targets cannot use ONLY");
        }
        if self.indexed.is_some() || self.not_indexed {
            return writer.invalid(
                "INSERT target",
                "index hints are not legal on an INSERT target",
            );
        }
        self.name.emit(writer)?;
        if let Some(alias) = &self.alias {
            alias.emit(writer)?;
        }
        Ok(())
    }
}

impl crate::ast::ReturningOption {
    pub(in crate::ast) fn emit_options(
        options: &[Self],
        writer: &mut SqlWriter,
    ) -> Result<(), super::DeparseError> {
        if !options.is_empty() {
            writer.postgres_only("RETURNING row aliases")?;
            writer.sql("WITH (");
            for (index, option) in options.iter().enumerate() {
                if index != 0 {
                    writer.sql(", ");
                }
                writer.sql(if option.old { "OLD AS " } else { "NEW AS " });
                writer.ident(&option.alias)?;
            }
            writer.sql(") ");
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_returning_options {
    #[rstest::rstest]
    #[case::insert("INSERT INTO t VALUES (1) RETURNING WITH (OLD AS o, NEW AS n) o.id, n.id")]
    #[case::update("UPDATE t SET id = 1 RETURNING WITH (NEW AS n, OLD AS o) o.id, n.id")]
    #[case::delete("DELETE FROM t RETURNING WITH (OLD AS prior) prior.*")]
    #[case::merge(
        "MERGE INTO t USING s ON t.id=s.id WHEN MATCHED THEN DELETE RETURNING WITH (OLD AS o) o.id"
    )]
    #[case::nested(
        "WITH changed AS (DELETE FROM t RETURNING WITH (OLD AS o) o.id) SELECT * FROM changed"
    )]
    fn preserves_row_aliases_and_expressions(#[case] sql: &str) {
        crate::test_helpers::assert_postgres_structure(sql);
    }
}
