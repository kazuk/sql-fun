use crate::{
    ast::{CurrentOf, Expr},
    context::ParseContext,
    names::TableName,
    sem::{
        AnalysisError, AnalysisProblem, CursorInfo, SemScalarExpr,
        problem::DeleteProblemKind,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// A positioned DELETE's portal reference and lookup result in its session scope.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeleteCursor {
    name: String,
    binding: Option<CursorInfo>,
    parameter: Option<i32>,
}
impl DeleteCursor {
    pub(crate) fn new(name: &str, binding: Option<&CursorInfo>) -> Self {
        Self {
            name: name.to_owned(),
            binding: binding.cloned(),
            parameter: None,
        }
    }
    pub(crate) const fn parameter(number: i32) -> Self {
        Self {
            name: String::new(),
            binding: None,
            parameter: Some(number),
        }
    }
    /// Procedural cursor parameter number, when static name lookup is unavailable.
    pub const fn parameter_number(&self) -> Option<i32> {
        self.parameter
    }
    /// Portal name retained from CURRENT OF, after SQL identifier normalization.
    /// Empty for a procedural parameter; inspect `parameter_number` in that case.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Resolved static facts, or None for a missing current-session binding.
    /// A binding with unknown facts is still a resolved reference.
    pub const fn binding(&self) -> Option<&CursorInfo> {
        self.binding.as_ref()
    }
}

/// Predicate alternatives are exclusive, as in PostgreSQL's DELETE grammar.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum DeletePredicate {
    Expression(Box<SemScalarExpr>),
    CurrentOf(DeleteCursor),
}
impl DeletePredicate {
    pub(crate) fn expression(value: &SemScalarExpr) -> Self {
        Self::Expression(Box::new(value.clone()))
    }
    pub(crate) fn current_of(value: &DeleteCursor) -> Self {
        Self::CurrentOf(value.clone())
    }
    pub(crate) fn as_expression(&self) -> Option<&SemScalarExpr> {
        match self {
            Self::Expression(value) => Some(value),
            Self::CurrentOf(_) => None,
        }
    }
    pub(crate) const fn as_cursor(&self) -> Option<&DeleteCursor> {
        match self {
            Self::CurrentOf(cursor) => Some(cursor),
            Self::Expression(_) => None,
        }
    }
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Expression(value) => {
                value.walk(visitor)?;
                visitor.predicate(value);
                Ok(())
            }
            Self::CurrentOf(_) => Ok(()),
        }
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        match self {
            Self::Expression(value) => {
                value.visit(visitor)?;
                visitor.predicate(value);
                Ok(())
            }
            Self::CurrentOf(_) => Ok(()),
        }
    }
}

impl DeleteCursor {
    pub(crate) fn from_ast_delete_current_of(
        ast: &Expr,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Option<Self>, AnalysisError> {
        match ast {
            Expr::CurrentOf(cursor) => Self::from_ast(cursor, context, target).map(Some),
            _ => Ok(None),
        }
    }
}

impl DeleteCursor {
    pub(crate) fn from_ast(
        ast: &CurrentOf,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Self, AnalysisError> {
        if ast.parameter() != 0 {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::cursor_parameter(ast.parameter()),
            ))?;
            return Ok(Self::parameter(ast.parameter()));
        }
        let cursor = Self::new(ast.name(), context.resolve_cursor(ast.name()));
        let problem = match cursor.binding() {
            None => Some(DeleteProblemKind::missing_cursor(ast.name())),
            Some(info) if info.updatable() == Some(false) => {
                Some(DeleteProblemKind::non_updatable_cursor(ast.name()))
            }
            Some(info) => info
                .target()
                .filter(|relation| *relation != target)
                .map(|actual| {
                    DeleteProblemKind::cursor_target_mismatch(ast.name(), target, actual)
                }),
        };
        if let Some(kind) = problem {
            context.report_analysis_problem(AnalysisProblem::delete(kind))?;
        }
        Ok(cursor)
    }
}

#[cfg(test)]
mod test_resolves_portal_in_current_context {
    use crate::{
        ast::{Dialect, StatementNode},
        context::ParseContext,
        names::{FullName, TableName},
        parse_statements,
        sem::{CursorInfo, SemAst},
        test_helpers::{
            create_cache_context, delete::catalog, statements::resolve_cursor as resolve,
        },
    };

    #[rstest::rstest]
    #[case::target(Some("items"), Some(true), false)]
    #[case::unknown_target(None, Some(true), false)]
    #[case::unknown_eligibility(Some("items"), None, false)]
    #[case::unknown_facts(None, None, false)]
    #[case::wrong_target(Some("other"), Some(true), true)]
    #[case::read_only(Some("items"), Some(false), true)]
    fn resolves_portal_in_current_context(
        #[case] target: Option<&str>,
        #[case] updatable: Option<bool>,
        #[case] invalid: bool,
    ) {
        let mut context = catalog(Dialect::postgres());
        let target = target.map(|name| TableName::from(FullName::with_schema("public", name)));
        let info = CursorInfo::new(target.as_ref(), updatable);
        context.bind_cursor("portal", &info);
        let statement = resolve(
            "DELETE FROM items AS i WHERE CURRENT OF portal RETURNING $1::integer",
            &mut context,
        );
        let delete = statement.as_delete().unwrap();
        assert!(delete.predicate().is_none());
        let cursor = delete.current_of().unwrap();
        assert_eq!(cursor.name(), "portal");
        assert_eq!(cursor.binding(), Some(&info));
        assert_eq!(
            context.reported_problem_count() > 0,
            invalid,
            "{:?}",
            context.reported_problems()
        );
        if updatable == Some(false) {
            assert_eq!(
                context.reported_problems()[0]
                    .delete_problem()
                    .unwrap()
                    .kind(),
                &crate::sem::problem::DeleteProblemKind::non_updatable_cursor("portal")
            );
        } else if invalid {
            assert_eq!(
                context.reported_problems()[0]
                    .delete_problem()
                    .unwrap()
                    .kind(),
                &crate::sem::problem::DeleteProblemKind::cursor_target_mismatch(
                    "portal",
                    delete.target(),
                    target.as_ref().unwrap()
                )
            );
        }
        assert!(
            context
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .allows(&crate::sem::PgBuiltInType::int4())
        );
    }

    #[rstest::rstest]
    #[case::empty_scope(false)]
    #[case::different_portal(true)]
    fn retains_missing_reference(#[case] other_binding: bool) {
        let mut context = catalog(Dialect::postgres());
        if other_binding {
            context.bind_cursor("other", &CursorInfo::new(None, None));
        }
        let statement = resolve("DELETE FROM items WHERE CURRENT OF absent", &mut context);
        let cursor = statement.as_delete().unwrap().current_of().unwrap();
        assert_eq!(cursor.name(), "absent");
        assert!(cursor.binding().is_none());
        assert_eq!(context.reported_problem_count(), 1);
        assert_eq!(
            context.reported_problems()[0]
                .delete_problem()
                .unwrap()
                .kind(),
            &crate::sem::problem::DeleteProblemKind::missing_cursor("absent")
        );
    }

    #[test]
    fn portal_scope_survives_statement_reset_and_can_be_replaced() {
        let mut context = catalog(Dialect::postgres());
        context.bind_cursor("portal", &CursorInfo::new(None, None));
        let first = resolve("DELETE FROM items WHERE CURRENT OF portal", &mut context);
        let second = resolve("DELETE FROM items WHERE CURRENT OF portal", &mut context);
        assert!(
            first
                .as_delete()
                .unwrap()
                .current_of()
                .unwrap()
                .binding()
                .is_some()
        );
        assert!(
            second
                .as_delete()
                .unwrap()
                .current_of()
                .unwrap()
                .binding()
                .is_some()
        );
        let replacement = CursorInfo::new(None, Some(false));
        context.bind_cursor("portal", &replacement);
        let third = resolve("DELETE FROM items WHERE CURRENT OF portal", &mut context);
        assert_eq!(
            third.as_delete().unwrap().current_of().unwrap().binding(),
            Some(&replacement)
        );
        // Semantic results are snapshots, independent of subsequent scope changes.
        assert_eq!(
            first
                .as_delete()
                .unwrap()
                .current_of()
                .unwrap()
                .binding()
                .unwrap()
                .updatable(),
            None
        );
        context.remove_cursor("portal");
        let last = resolve("DELETE FROM items WHERE CURRENT OF portal", &mut context);
        assert!(
            last.as_delete()
                .unwrap()
                .current_of()
                .unwrap()
                .binding()
                .is_none()
        );
    }

    #[rstest::rstest]
    #[case::quoted(r#"DELETE FROM items WHERE CURRENT OF "Mixed""#, "Mixed", false)]
    #[case::unquoted("DELETE FROM items WHERE CURRENT OF Mixed", "mixed", true)]
    fn portal_names_follow_identifier_rules(
        #[case] sql: &str,
        #[case] name: &str,
        #[case] missing: bool,
    ) {
        let mut context = catalog(Dialect::postgres());
        context.bind_cursor("Mixed", &CursorInfo::new(None, None));
        let statement = resolve(sql, &mut context);
        let cursor = statement.as_delete().unwrap().current_of().unwrap();
        assert_eq!(cursor.name(), name);
        assert_eq!(cursor.binding().is_none(), missing);
    }

    #[test]
    fn retains_unsupported_procedural_cursor_parameter() {
        let mut context = catalog(Dialect::postgres());
        let mut node = parse_statements(
            "DELETE FROM items WHERE CURRENT OF portal",
            Dialect::postgres(),
        )
        .next()
        .unwrap()
        .unwrap()
        .into_test_node();
        let StatementNode::Delete(delete) = &mut node else {
            panic!("DELETE")
        };
        delete.set_current_of_parameter(3);
        let result =
            SemAst::from_ast_node(&node, &mut context, &mut create_cache_context()).unwrap();
        assert_eq!(
            result
                .as_delete()
                .unwrap()
                .current_of()
                .unwrap()
                .parameter_number(),
            Some(3)
        );
        assert_eq!(
            context.reported_problems()[0]
                .delete_problem()
                .unwrap()
                .kind(),
            &crate::sem::problem::DeleteProblemKind::cursor_parameter(3)
        );
    }
}
