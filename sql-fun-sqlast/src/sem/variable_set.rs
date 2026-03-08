use sql_fun_core::IVec;

use crate::{
    sem::{AstAndContextPair, FromClause, SemScalarExpr, WithClause},
    syn::{ListOpt, ScanToken},
};

use super::{AnalysisError, ParseContext, SemAst};

/// set a variable
#[derive(Debug, Clone)]
pub struct VariableSet {
    name: String,
    value: SemScalarExpr,
}

impl VariableSet {
    /// name of variable
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// value of variable
    #[must_use]
    pub fn value(&self) -> &SemScalarExpr {
        &self.value
    }

    /// analyze `VariableSetStmt`
    ///
    /// # Errors
    ///
    /// Returns [`AnalysisError`] when the variable assignment is semantically invalid.
    pub fn analyze_variable_set<TParseContext>(
        mut context: TParseContext,
        syn: &crate::syn::VariableSetStmt,
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
            &WithClause::default(),
            &FromClause::default(),
            args[0].clone(),
            tokens,
        )?;
        context = new_context;

        let var_set = VariableSet { name, value };
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
    syn: &crate::syn::VariableSetStmt,
    tokens: &IVec<ScanToken>,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    VariableSet::analyze_variable_set(context, syn, tokens)
}
