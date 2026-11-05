use crate::sem::{
    AnalysisError, SelectStatement, TypeReference,
    scalar_expr::SemScalarExprNode,
    traversal::{SemanticVisitor, SemanticVisitorMut},
};

/// Sub-query expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SubLinkExpr {
    subselect: Box<SelectStatement>,
    type_ref: TypeReference,
}

impl SubLinkExpr {
    pub(super) fn output_name(&self) -> String {
        let columns = self.subselect.result_columns();
        if columns.len() == 1
            && let Some(column) = columns.get_column_at(0)
            && let Some(name) = column.column_name()
        {
            return name.to_string();
        }
        String::new()
    }

    pub(super) fn from_resolved(query: &SelectStatement) -> Result<Self, AnalysisError> {
        Ok(Self {
            subselect: Box::new(query.clone()),
            type_ref: query.returning_composite_type()?,
        })
    }
}

impl SemScalarExprNode for SubLinkExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.subselect.result_columns().scalar_type()
    }

    fn is_not_null(&self) -> Option<bool> {
        Some(false)
    }
}

#[cfg(test)]
mod test_concrete_scalar_subquery_exposes_its_scalar_type {
    use crate::{ast::Dialect, sem::PgBuiltInType, test_helpers::public_analysis::analyze};

    #[test]
    fn concrete_scalar_subquery_exposes_its_scalar_type() {
        let result = analyze("select (select 1) + 1 as n", Dialect::postgres()).unwrap();
        assert_eq!(result.context().reported_problem_count(), 0);
        let query = result.ast().clone().into_select_statemtnt().unwrap();
        assert_eq!(
            query.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::int4())
        );
    }
}

#[cfg(test)]
mod test_scalar_subquery_remains_nullable_integer {
    use crate::{
        ast::Dialect,
        names::ColumnName,
        sem::PgBuiltInType,
        test_helpers::query::{expression as projection, resolve},
    };

    #[test]
    fn scalar_subquery_remains_nullable_integer() {
        let (query, context) =
            resolve("SELECT (SELECT 1 AS n) AS value", Dialect::postgres(), true).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        let column = query
            .get_result_column_def(&ColumnName::from("value"))
            .unwrap();
        assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::int4()));
        assert_eq!(column.is_not_null(), Some(false));
        assert!(projection(&query).as_exists().is_none());
    }
}

impl SubLinkExpr {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.subselect.walk(visitor)?;
        self.type_ref = self.subselect.returning_composite_type()?;
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.subselect.visit(visitor)?;
        Ok(())
    }
}
