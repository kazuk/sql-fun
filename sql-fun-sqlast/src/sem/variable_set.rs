use crate::{
    ast::VariableSet as AstVariableSet,
    context::ParseContext,
    sem::{AnalysisError, AstAndContextPair, ScalarAnalysisContext, SemAst, SemScalarExpr},
    syn::{ListOpt, ScanToken, VariableSetStmt},
};
use sql_fun_core::IVec;

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

    /// analyze `VariableSetStmt`
    ///
    /// # Errors
    ///
    /// Returns [`AnalysisError`] when the variable assignment is semantically invalid.
    pub fn analyze_variable_set<TParseContext>(
        mut context: TParseContext,
        syn: &VariableSetStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let name = syn.get_name();
        let Some(args) = syn.get_args().as_inner() else {
            AnalysisError::raise_unexpected_none("VariableSetStmt.args")?
        };
        if args.len() != 1 {
            AnalysisError::raise_unexpected_input("VariableSetStmt.args len!=1")?;
        }

        let (value, new_context) = SemScalarExpr::analyze_scaler_expr(
            context,
            &ScalarAnalysisContext::default(),
            args[0].clone(),
            tokens,
        )?;
        context = new_context;

        let var_set = Self::new(&name, &value);
        let result_context = context.apply_variable_set(&var_set)?;
        Ok(AstAndContextPair::new(
            SemAst::VariableSet(var_set),
            result_context,
        ))
    }
}

/// analyze `VariableSetStmt`
///
/// # Errors
///
/// Returns [`AnalysisError`] when the variable assignment is semantically invalid.
#[deprecated = "use VariableSet::analyze_variable_set"]
pub fn analyze_variable_set<TParseContext>(
    context: TParseContext,
    syn: &VariableSetStmt,
    tokens: &IVec<ScanToken>,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    VariableSet::analyze_variable_set(context, syn, tokens)
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
        let [argument] = ast.arguments.as_slice() else {
            return AnalysisError::raise_unexpected_input("VariableSetStmt.args len!=1");
        };
        let outcome =
            SemScalarExpr::from_ast(argument, &mut context, &ScalarAnalysisContext::default());
        let value = outcome?;
        let variable_set = Self::new(&ast.name, &value);
        let context = context.apply_variable_set(&variable_set)?;
        Ok(AstAndContextPair::new(
            SemAst::from_variable_set(&variable_set),
            context,
        ))
    }
}
