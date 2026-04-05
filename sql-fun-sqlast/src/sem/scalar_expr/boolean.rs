use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, FromClause, ParseContext, PgBuiltInType, SemScalarExpr, TypeReference,
        WithClause,
    },
    syn::{BoolExprType, ListOpt, Opt, ScanToken},
};

use super::{AnalyzeScalarExpr, SemScalarExprNode};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BooleanExprBody {
    And(Vec<SemScalarExpr>),
    Or(Vec<SemScalarExpr>),
    Not(Box<SemScalarExpr>),
}

impl BooleanExprBody {
    pub fn and(items: &[SemScalarExpr]) -> Self {
        Self::And(items.to_vec())
    }

    pub fn or(items: &[SemScalarExpr]) -> Self {
        Self::Or(items.to_vec())
    }

    pub fn not(expr: &SemScalarExpr) -> Self {
        Self::Not(Box::new(expr.clone()))
    }
}

/// boolean expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BooleanExpr {
    body: BooleanExprBody,
}

impl BooleanExpr {
    /// create boolean expression from body
    pub fn new(body: BooleanExprBody) -> Self {
        Self { body }
    }

    fn analyze_args<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        nodes: Vec<crate::syn::Node>,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Vec<SemScalarExpr>, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let mut results = Vec::new();
        for node in nodes {
            let (arg_expr, new_context) = SemScalarExpr::analyze_scaler_expr(
                context,
                with_clause,
                from_clause,
                node,
                tokens,
            )?;
            context = new_context;
            results.push(arg_expr);
        }
        Ok((results, context))
    }
}

impl SemScalarExprNode for BooleanExpr {
    fn get_type(&self) -> Option<TypeReference> {
        Some(PgBuiltInType::bool())
    }

    fn is_not_null(&self) -> Option<bool> {
        match &self.body {
            BooleanExprBody::And(items) => {
                for item in items {
                    let is_not_null = item.is_not_null()?;
                    if !is_not_null {
                        return Some(false);
                    }
                }
                Some(true)
            }
            BooleanExprBody::Or(items) => {
                for item in items {
                    let is_not_null = item.is_not_null()?;
                    if !is_not_null {
                        return Some(false);
                    }
                }
                Some(true)
            }
            BooleanExprBody::Not(expr) => expr.is_not_null(),
        }
    }
}

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::BoolExpr> for BooleanExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: crate::syn::BoolExpr,
        tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(boolop) = syn.get_boolop().as_inner() else {
            AnalysisError::raise_unexpected_none("boolexpr.boolop")?
        };
        let Some(args) = syn.get_args().as_inner() else {
            AnalysisError::raise_unexpected_none("boolexpr.args")?
        };
        let (args, new_context) =
            Self::analyze_args(context, with_clause, from_clause, args, tokens)?;
        context = new_context;

        let body = match boolop {
            BoolExprType::AndExpr => BooleanExprBody::and(&args),
            BoolExprType::OrExpr => BooleanExprBody::or(&args),
            BoolExprType::NotExpr => {
                if args.len() != 1 {
                    AnalysisError::raise_unexpected_input("boolexpr.args len")?;
                }
                BooleanExprBody::not(&args[0])
            }
            _ => AnalysisError::raise_unexpected_input("boolexpr.boolop unexpected value")?,
        };
        let expr = Self { body };
        Ok((SemScalarExpr::Boolean(expr), context))
    }
}

/// `NullTestExpr`
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NullTestExpr {
    null_test_type: NullTestType,
    arg_expr: Box<SemScalarExpr>,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NullTestType {
    IsNull,
    IsNotNull,
}

impl NullTestExpr {
    /// create null test expression from semantic input expression
    pub fn new(null_test_type: NullTestType, arg_expr: SemScalarExpr) -> Self {
        Self {
            null_test_type,
            arg_expr: Box::new(arg_expr),
        }
    }
}

impl TryFrom<crate::syn::NullTestType> for NullTestType {
    type Error = AnalysisError;

    fn try_from(value: crate::syn::NullTestType) -> Result<Self, Self::Error> {
        match value {
            crate::syn::NullTestType::OptionNone | crate::syn::NullTestType::Undefined => {
                AnalysisError::raise_unexpected_input("nulltesttype")?
            }
            crate::syn::NullTestType::IsNull => Ok(Self::IsNull),
            crate::syn::NullTestType::IsNotNull => Ok(Self::IsNotNull),
        }
    }
}

impl SemScalarExprNode for NullTestExpr {
    fn get_type(&self) -> Option<TypeReference> {
        Some(PgBuiltInType::bool())
    }

    fn is_not_null(&self) -> Option<bool> {
        Some(true)
    }
}

impl<TParseContext> AnalyzeScalarExpr<TParseContext, crate::syn::NullTest> for NullTestExpr
where
    TParseContext: ParseContext,
{
    fn analyze_scalar_expr(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        syn: crate::syn::NullTest,
        tokens: &IVec<ScanToken>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(null_test_type) = syn.get_nulltesttype().as_inner() else {
            AnalysisError::raise_unexpected_none("NullTest.nulltesttype")?
        };
        let Some(arg) = syn.get_arg().as_inner() else {
            AnalysisError::raise_unexpected_none("NullTest.arg")?
        };
        let (arg_expr, new_context) =
            SemScalarExpr::analyze_scaler_expr(context, with_clause, from_clause, arg, tokens)?;
        context = new_context;
        Ok((
            SemScalarExpr::NullTest(Self {
                null_test_type: NullTestType::try_from(null_test_type)?,
                arg_expr: Box::new(arg_expr),
            }),
            context,
        ))
    }
}
