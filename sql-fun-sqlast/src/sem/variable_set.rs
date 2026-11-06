use crate::{
    ast::VariableSet as AstVariableSet,
    context::ParseContext,
    sem::{AnalysisError, AstAndContextPair, ScalarAnalysisContext, SemAst, SemScalarExpr},
};

/// set a variable
#[derive(Debug, Clone)]
pub struct VariableSet {
    name: String,
    value: SemScalarExpr,
}

impl VariableSet {
    pub(crate) fn new(name: &str, value: &SemScalarExpr) -> Self {
        Self {
            name: name.to_owned(),
            value: value.clone(),
        }
    }

    /// name of variable
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// value of variable
    #[must_use]
    pub const fn value(&self) -> &SemScalarExpr {
        &self.value
    }
}

// Analysis owned by the semantic definitions in this module.

impl VariableSet {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &AstVariableSet,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let [argument] = ast.arguments().as_slice() else {
            return AnalysisError::raise_unexpected_input("VariableSetStmt.args len!=1");
        };
        let outcome =
            SemScalarExpr::from_ast(argument, &mut context, &ScalarAnalysisContext::default());
        let value = outcome?;
        let variable_set = Self::new(ast.name(), &value);
        let context = context.apply_variable_set(&variable_set)?;
        Ok(AstAndContextPair::new(
            SemAst::from_variable_set(&variable_set),
            context,
        ))
    }
}
