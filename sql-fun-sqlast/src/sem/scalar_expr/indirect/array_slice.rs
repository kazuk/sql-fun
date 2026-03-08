use sql_fun_core::IVec;

use super::IndirectionExpr;
use crate::{
    sem::{AnalysisError, FromClause, ParseContext, SemScalarExpr, TypeReference, WithClause},
    syn::{Opt, ScanToken},
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArraySlice {
    /// array
    array: SemScalarExpr,
    /// lower bound
    lower: Option<SemScalarExpr>,
    /// upper bound
    upper: Option<SemScalarExpr>,
}

impl ArraySlice {
    pub fn new(
        array: SemScalarExpr,
        lower: Option<SemScalarExpr>,
        upper: Option<SemScalarExpr>,
    ) -> Self {
        Self {
            array,
            lower,
            upper,
        }
    }

    pub fn array_type(&self) -> Option<TypeReference> {
        self.array.get_type()
    }

    pub fn array_is_not_null(&self) -> Option<bool> {
        self.array.is_not_null()
    }

    pub fn array_slice<TParseContext>(
        array: SemScalarExpr,
        lower: Option<SemScalarExpr>,
        upper: Option<SemScalarExpr>,
        context: &mut TParseContext,
    ) -> Result<IndirectionExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        array.require_array(context)?;
        if let Some(lower_index) = &lower {
            lower_index.fits_array_index(context)?;
        }
        if let Some(upper_index) = &upper {
            upper_index.fits_array_index(context)?;
        }

        Ok(IndirectionExpr::ArraySlice(Box::new(Self::new(
            array, lower, upper,
        ))))
    }

    fn analyze_index<TParseContext>(
        context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        index: crate::syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Option<SemScalarExpr>, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(index) = index.as_inner() else {
            return Ok((None, context));
        };
        let (lexpr, new_context) =
            SemScalarExpr::analyze_scaler_expr(context, with_clause, from_clause, index, tokens)?;
        Ok((Some(lexpr), new_context))
    }

    pub fn analyze_slice<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        arg: SemScalarExpr,
        indicies: crate::syn::AIndices,
        tokens: &IVec<ScanToken>,
    ) -> Result<(IndirectionExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let (lexpr, new_context) = Self::analyze_index(
            context,
            with_clause,
            from_clause,
            indicies.get_lidx(),
            tokens,
        )?;
        context = new_context;
        let (rexpr, new_context) = Self::analyze_index(
            context,
            with_clause,
            from_clause,
            indicies.get_uidx(),
            tokens,
        )?;
        context = new_context;
        let arg = Self::array_slice(arg, lexpr, rexpr, &mut context)?;
        Ok((arg, context))
    }
}
