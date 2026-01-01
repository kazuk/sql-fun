use crate::{
    sem::{AstAndContextPair, FromClause, SemScalarExpr, WithClause, analyze_scaler_expr},
    syn::ListOpt,
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
}

/// analyze `VariableSetStmt`
pub fn analyze_variable_set<TParseContext>(
    mut context: TParseContext,
    syn: &crate::syn::VariableSetStmt,
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

    let (value, new_context) = analyze_scaler_expr(
        context,
        &WithClause::default(),
        &FromClause::default(),
        args[0].clone(),
    )?;
    context = new_context;

    let var_set = VariableSet { name, value };
    let result_context = context.apply_variable_set(&var_set)?;
    Ok(AstAndContextPair::new(
        SemAst::VariableSet(var_set),
        result_context,
    ))
}
