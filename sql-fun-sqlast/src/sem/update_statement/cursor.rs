use crate::{
    ast::{CurrentOf, Expr},
    context::ParseContext,
    names::TableName,
    sem::{
        AnalysisError, AnalysisProblem, CursorInfo, ResolutionFailure, SemScalarExpr,
        problem::UpdateProblemKind,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// A positioned UPDATE's portal reference and lookup result in its session scope.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UpdateCursor {
    name: String,
    binding: Option<CursorInfo>,
}
impl UpdateCursor {
    pub(crate) fn new(name: &str, binding: Option<&CursorInfo>) -> Self {
        Self {
            name: name.to_owned(),
            binding: binding.cloned(),
        }
    }
    /// Portal name retained from CURRENT OF, after SQL identifier normalization.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Resolved static facts, or None for a missing current-session binding.
    /// A binding with unknown facts is still a resolved reference.
    pub const fn binding(&self) -> Option<&CursorInfo> {
        self.binding.as_ref()
    }
}

/// Predicate alternatives are exclusive, as in PostgreSQL's UPDATE grammar.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum UpdatePredicate {
    Expression(Box<SemScalarExpr>),
    CurrentOf(UpdateCursor),
}
impl UpdatePredicate {
    pub(crate) fn expression(value: &SemScalarExpr) -> Self {
        Self::Expression(Box::new(value.clone()))
    }
    pub(crate) fn current_of(value: &UpdateCursor) -> Self {
        Self::CurrentOf(value.clone())
    }
    pub(crate) fn as_expression(&self) -> Option<&SemScalarExpr> {
        match self {
            Self::Expression(value) => Some(value),
            Self::CurrentOf(_) => None,
        }
    }
    pub(crate) const fn as_cursor(&self) -> Option<&UpdateCursor> {
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

impl UpdateCursor {
    pub(crate) fn from_ast_current_of(
        ast: &Expr,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Option<Self>, ResolutionFailure> {
        match ast {
            Expr::CurrentOf(cursor) => Self::from_ast(cursor, context, target).map(Some),
            _ => Ok(None),
        }
    }
}

impl UpdateCursor {
    pub(crate) fn from_ast(
        ast: &CurrentOf,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Self, ResolutionFailure> {
        if ast.parameter() != 0 {
            return Err(AnalysisError::unexpected_input_error(
                "CURRENT OF cursor parameters require a procedural variable resolver",
            )
            .into());
        }
        let cursor = Self::new(ast.name(), context.resolve_cursor(ast.name()));
        let problem = match cursor.binding() {
            None => Some(format!(
                "cursor {} does not exist in the current session scope",
                crate::identifier::QuotedIdentifier::from(ast.name().as_str())
            )),
            Some(info) if info.updatable() == Some(false) => Some(format!(
                "cursor {} is not statically updatable",
                crate::identifier::QuotedIdentifier::from(ast.name().as_str())
            )),
            Some(info) if info.target().is_some_and(|relation| relation != target) => {
                Some(format!(
                    "cursor {} does not target {}",
                    crate::identifier::QuotedIdentifier::from(ast.name().as_str()),
                    target
                ))
            }
            Some(_) => None,
        };
        if let Some(message) = problem {
            context.report_analysis_problem(AnalysisProblem::update(
                UpdateProblemKind::cursor(),
                &message,
            ))?;
        }
        Ok(cursor)
    }
}

#[cfg(test)]
mod test_resolves_portal_in_current_context {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        names::{FullName, TableName},
        sem::CursorInfo,
        test_helpers::{statements::resolve_cursor as resolve, update::catalog},
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
            "UPDATE items AS i SET id=$1 WHERE CURRENT OF portal RETURNING i.id",
            &mut context,
        );
        let update = statement.as_update().unwrap();
        assert!(update.predicate().is_none());
        let cursor = update.current_of().unwrap();
        assert_eq!(cursor.name(), "portal");
        assert_eq!(cursor.binding(), Some(&info));
        assert_eq!(
            context.reported_problem_count() > 0,
            invalid,
            "{:?}",
            context.reported_problems()
        );
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
        let statement = resolve(
            "UPDATE items SET id=1 WHERE CURRENT OF absent",
            &mut context,
        );
        let cursor = statement.as_update().unwrap().current_of().unwrap();
        assert_eq!(cursor.name(), "absent");
        assert!(cursor.binding().is_none());
        assert_eq!(context.reported_problem_count(), 1);
        assert!(context.reported_problems()[0].update_problem().is_some());
    }

    #[test]
    fn portal_scope_survives_statement_reset_and_can_be_replaced() {
        let mut context = catalog(Dialect::postgres());
        context.bind_cursor("portal", &CursorInfo::new(None, None));
        let first = resolve(
            "UPDATE items SET id=1 WHERE CURRENT OF portal",
            &mut context,
        );
        let second = resolve(
            "UPDATE items SET id=2 WHERE CURRENT OF portal",
            &mut context,
        );
        assert!(
            first
                .as_update()
                .unwrap()
                .current_of()
                .unwrap()
                .binding()
                .is_some()
        );
        assert!(
            second
                .as_update()
                .unwrap()
                .current_of()
                .unwrap()
                .binding()
                .is_some()
        );
        let replacement = CursorInfo::new(None, Some(false));
        context.bind_cursor("portal", &replacement);
        let third = resolve(
            "UPDATE items SET id=3 WHERE CURRENT OF portal",
            &mut context,
        );
        assert_eq!(
            third.as_update().unwrap().current_of().unwrap().binding(),
            Some(&replacement)
        );
        // Semantic results are snapshots, independent of subsequent scope changes.
        assert_eq!(
            first
                .as_update()
                .unwrap()
                .current_of()
                .unwrap()
                .binding()
                .unwrap()
                .updatable(),
            None
        );
        context.remove_cursor("portal");
        let last = resolve(
            "UPDATE items SET id=4 WHERE CURRENT OF portal",
            &mut context,
        );
        assert!(
            last.as_update()
                .unwrap()
                .current_of()
                .unwrap()
                .binding()
                .is_none()
        );
    }

    #[rstest::rstest]
    #[case::quoted(r#"UPDATE items SET id=1 WHERE CURRENT OF "Mixed""#, "Mixed", false)]
    #[case::unquoted("UPDATE items SET id=1 WHERE CURRENT OF Mixed", "mixed", true)]
    fn portal_names_follow_identifier_rules(
        #[case] sql: &str,
        #[case] name: &str,
        #[case] missing: bool,
    ) {
        let mut context = catalog(Dialect::postgres());
        context.bind_cursor("Mixed", &CursorInfo::new(None, None));
        let statement = resolve(sql, &mut context);
        let cursor = statement.as_update().unwrap().current_of().unwrap();
        assert_eq!(cursor.name(), name);
        assert_eq!(cursor.binding().is_none(), missing);
    }
}
