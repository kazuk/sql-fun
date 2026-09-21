use sql_fun_core::IVec;

use super::IndirectionExpr;
use crate::{
    sem::{AnalysisError, ParseContext, ScalarAnalysisContext, SemScalarExpr, TypeReference},
    syn::{Opt, ScanToken},
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArrayIndex {
    array: SemScalarExpr,
    index: SemScalarExpr,
    item_type: Option<TypeReference>,
}

impl ArrayIndex {
    pub const fn new(
        array: SemScalarExpr,
        index: SemScalarExpr,
        item_type: Option<TypeReference>,
    ) -> Self {
        Self {
            array,
            index,
            item_type,
        }
    }

    pub const fn array(&self) -> &SemScalarExpr {
        &self.array
    }

    pub const fn item_type(&self) -> &Option<TypeReference> {
        &self.item_type
    }

    pub fn array_index<TParseContext>(
        array: SemScalarExpr,
        index: SemScalarExpr,
        context: &mut TParseContext,
    ) -> Result<IndirectionExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        array.require_array(context)?;
        index.fits_array_index(context)?;
        let item_type = array
            .get_type()
            .and_then(|v| context.get_array_item_type(&v));

        Ok(IndirectionExpr::ArrayIndex(Box::new(Self::new(
            array, index, item_type,
        ))))
    }

    pub fn analyze_index<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        arg: SemScalarExpr,
        indices: &crate::syn::AIndices,
        tokens: &IVec<ScanToken>,
    ) -> Result<(IndirectionExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let Some(uidx) = indices.get_uidx().as_inner() else {
            AnalysisError::raise_unexpected_none("aindicies.uidx")?
        };
        let (rexpr, new_context) =
            SemScalarExpr::analyze_scaler_expr(context, scalar_context, uidx, tokens)?;
        context = new_context;
        let arg = Self::array_index(arg, rexpr, &mut context)?;
        Ok((arg, context))
    }
}
