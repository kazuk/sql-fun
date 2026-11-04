use crate::{
    ast::Dialect,
    sem::{
        AnalysisError, InputTypeConstraint, ParseContext, PgBuiltInType, SemScalarExpr,
        TypeReference,
        parameter_resolution::ParameterResolution,
        scalar_expr::SemScalarExprNode,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
    syn::NullTestType as SynNullTestType,
};

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
    pub const fn new(body: BooleanExprBody) -> Self {
        Self { body }
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

impl NullTestType {
    pub(crate) const fn from_negated(negated: bool) -> Self {
        if negated {
            Self::IsNotNull
        } else {
            Self::IsNull
        }
    }
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

impl TryFrom<SynNullTestType> for NullTestType {
    type Error = AnalysisError;

    fn try_from(value: SynNullTestType) -> Result<Self, Self::Error> {
        match value {
            SynNullTestType::OptionNone | SynNullTestType::Undefined => {
                AnalysisError::raise_unexpected_input("nulltesttype")?
            }
            SynNullTestType::IsNull => Ok(Self::IsNull),
            SynNullTestType::IsNotNull => Ok(Self::IsNotNull),
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

impl BooleanExpr {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.body.walk(visitor)?;
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.body.visit(visitor)?;
        Ok(())
    }
}

impl BooleanExprBody {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::And(values) | Self::Or(values) => {
                for value in values {
                    visitor.boolean_input(value)?;
                    value.walk(visitor)?;
                }
            }
            Self::Not(value) => {
                visitor.boolean_input(value)?;
                value.walk(visitor)?;
            }
        }
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        match self {
            Self::And(values) | Self::Or(values) => {
                for value in values {
                    value.visit(visitor)?;
                }
            }
            Self::Not(value) => value.visit(visitor)?,
        }
        Ok(())
    }
}

impl NullTestExpr {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.arg_expr.walk(visitor)?;
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.arg_expr.visit(visitor)?;
        Ok(())
    }
}

impl BooleanExprBody {
    fn inference_arguments(&self) -> Vec<SemScalarExpr> {
        match self {
            Self::And(values) | Self::Or(values) => values.clone(),
            Self::Not(value) => vec![*value.clone()],
        }
    }
}
impl BooleanExpr {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut ParameterResolution<'_>,
        context: &impl ParseContext,
    ) -> Option<SemScalarExpr> {
        let arguments = self.body.inference_arguments();
        if !arguments.iter().any(|arg| arg.constraint_node().is_some()) {
            return None;
        }
        if context.dialect() == Dialect::postgres() {
            for arg in &arguments {
                parameters.require(context, arg, &PgBuiltInType::bool(), false);
            }
        }
        Some(parameters.operation(
            "boolean",
            &arguments,
            &InputTypeConstraint::exact(&PgBuiltInType::bool()),
            None,
        ))
    }
}
impl NullTestExpr {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut ParameterResolution<'_>,
    ) -> Option<SemScalarExpr> {
        self.arg_expr.constraint_node()?;
        Some(parameters.operation(
            "IS NULL",
            std::slice::from_ref(&self.arg_expr),
            &InputTypeConstraint::exact(&PgBuiltInType::bool()),
            Some(true),
        ))
    }
}
