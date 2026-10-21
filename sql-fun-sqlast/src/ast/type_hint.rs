//! Parse a type through the selected frontend without analyzing synthetic SQL.
#[cfg(test)]
mod tests;
use crate::ast::{Cast, Expr, Literal, Query, QueryBody, Select, StatementNode, TypeName};
use crate::sem::{AnalysisError, ParseContext, TypeReference};

impl TypeName {
    pub(crate) fn resolve_parameter_hint(
        hint: &str,
        context: &impl ParseContext,
    ) -> Result<TypeReference, AnalysisError> {
        // A newline prevents a trailing line comment from hiding the closing
        // delimiter. Check the complete wrapper shape before extracting the type.
        let sql = format!("SELECT CAST(NULL AS {hint}\n) AS __sql_fun_hint");
        let mut statements = crate::parse_statements(&sql, context.dialect());
        let statement = statements
            .next()
            .ok_or_else(invalid_hint)?
            .map_err(|error| AnalysisError::unexpected_input_error(&error.to_string()))?;
        if statements.next().is_some() {
            return Err(invalid_hint());
        }
        let ty = statement.node.into_hint_type()?;
        // Parameter declarations describe type identity. PostgreSQL PREPARE
        // does not enforce typmods (varchar length, numeric precision/scale,
        // timestamp precision) on inputs; SQLite type sizes likewise do not
        // constrain input values. The parser still validates their SQL syntax.
        if ty.set_of || ty.percent_type || !(1..=3).contains(&ty.name.0.len()) {
            return Err(invalid_hint());
        }
        let reference = ty.reference()?;
        context.canonicalize_type_ref(&reference).ok_or_else(|| {
            AnalysisError::unexpected_input_error(&format!("SQL type {reference} not found"))
        })
    }
}

impl StatementNode {
    fn into_hint_type(self) -> Result<TypeName, AnalysisError> {
        if let Self::Query(query) = self {
            query.into_hint_type()
        } else {
            Err(invalid_hint())
        }
    }
}

impl Query {
    fn into_hint_type(self) -> Result<TypeName, AnalysisError> {
        if !self.with.entries.is_empty()
            || !self.order.is_empty()
            || self.limit.is_some()
            || self.offset.is_some()
            || !self.locks.is_empty()
            || self.with_ties
        {
            return Err(invalid_hint());
        }
        self.body.into_hint_type()
    }
}

impl QueryBody {
    fn into_hint_type(self) -> Result<TypeName, AnalysisError> {
        if let Self::Select(select) = self {
            select.into_hint_type()
        } else {
            Err(invalid_hint())
        }
    }
}

impl Select {
    fn into_hint_type(mut self) -> Result<TypeName, AnalysisError> {
        if self.into.is_some()
            || self.distinct.is_some()
            || !self.from.is_empty()
            || self.predicate.is_some()
            || !self.group.is_empty()
            || self.group_distinct
            || self.having.is_some()
            || !self.windows.is_empty()
            || self.targets.len() != 1
        {
            return Err(invalid_hint());
        }
        let target = self.targets.pop().ok_or_else(invalid_hint)?;
        if target.alias.as_deref() != Some("__sql_fun_hint") {
            return Err(invalid_hint());
        }
        target.expression.into_hint_type()
    }
}

impl Expr {
    fn into_hint_type(self) -> Result<TypeName, AnalysisError> {
        if let Self::Cast(cast) = self {
            cast.into_hint_type()
        } else {
            Err(invalid_hint())
        }
    }

    const fn is_hint_null(&self) -> bool {
        if let Self::Literal(value) = self {
            value.is_hint_null()
        } else {
            false
        }
    }
}

impl Literal {
    const fn is_hint_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

impl Cast {
    fn into_hint_type(self) -> Result<TypeName, AnalysisError> {
        if self.argument.is_hint_null() {
            Ok(self.target)
        } else {
            Err(invalid_hint())
        }
    }
}

fn invalid_hint() -> AnalysisError {
    AnalysisError::unexpected_input_error("expected exactly one SQL type name")
}
