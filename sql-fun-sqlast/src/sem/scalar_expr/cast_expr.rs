#[cfg(feature = "sqlite")]
use crate::SqliteToken;
use crate::sem::scalar_expr::SemScalarExprNode;
#[cfg(feature = "sqlite")]
use crate::sem::{AnalysisError, ParseContext, ScalarAnalysisContext};
use crate::sem::{SemScalarExpr, TypeReference};

#[cfg(feature = "sqlite")]
use ::sql_fun_core::IVec;

/// implicit cast expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImplicitCastExpr {
    value: Box<SemScalarExpr>,
    cast_target: TypeReference,
}

impl ImplicitCastExpr {
    pub(crate) fn rename_relation(
        &mut self,
        old: &crate::sem::TableName,
        new: &crate::sem::TableName,
    ) {
        self.cast_target.rename_relation(old, new);
    }

    pub(crate) fn projection_operand(&self) -> (&SemScalarExpr, &TypeReference) {
        (&self.value, &self.cast_target)
    }

    /// create a implicit cast expression
    #[must_use]
    pub fn new(value: &SemScalarExpr, cast_target: &TypeReference) -> Self {
        Self {
            value: Box::new(value.clone()),
            cast_target: cast_target.clone(),
        }
    }
}

impl SemScalarExprNode for ImplicitCastExpr {
    fn get_type(&self) -> Option<TypeReference> {
        Some(self.cast_target.clone())
    }

    fn is_not_null(&self) -> Option<bool> {
        self.value.is_not_null()
    }
}

/// explicit cast expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TypeCastExpr {
    target_type: TypeReference,
    resolution: crate::sem::QueryNameResolution,
    arg: Box<SemScalarExpr>,
}

impl TypeCastExpr {
    pub(crate) fn rename_relation(
        &mut self,
        old: &crate::sem::TableName,
        new: &crate::sem::TableName,
    ) {
        self.target_type.rename_relation(old, new);
        self.resolution.rename_relation(old, new);
    }

    pub(crate) fn projection_operand(&self) -> (&SemScalarExpr, &TypeReference) {
        (&self.arg, &self.target_type)
    }

    pub(super) fn record_operand(&self) -> Option<&SemScalarExpr> {
        if self.target_type == TypeReference::from_master_type_name("record") {
            Some(&self.arg)
        } else {
            None
        }
    }

    pub(super) fn constructor_projection_name(&self) -> Option<&'static str> {
        self.arg.constructor_projection_name()
    }

    pub(super) fn grouping_key_operand(&self) -> Option<&SemScalarExpr> {
        if self.resolution.is_resolved() && self.arg.get_type().as_ref() == Some(&self.target_type)
        {
            Some(&self.arg)
        } else {
            None
        }
    }

    pub(crate) fn set_resolution(&mut self, resolution: crate::sem::QueryNameResolution) {
        self.resolution = resolution;
    }
    /// Completed target-name resolution (or SQLite affinity interpretation).
    pub const fn resolution(&self) -> &crate::sem::QueryNameResolution {
        &self.resolution
    }

    pub(super) fn from_resolved(target_type: &TypeReference, arg: &SemScalarExpr) -> Self {
        Self {
            target_type: target_type.clone(),
            resolution: crate::sem::QueryNameResolution::type_reference(target_type),
            arg: Box::new(arg.clone()),
        }
    }
}

impl SemScalarExprNode for TypeCastExpr {
    fn get_type(&self) -> Option<TypeReference> {
        Some(self.target_type.clone())
    }

    fn is_not_null(&self) -> Option<bool> {
        self.arg.is_not_null()
    }
}

#[cfg(feature = "sqlite")]
impl TypeCastExpr {
    /// analyze [`crate::syn::sqlite::ExprCast`]
    pub fn analyze_sqlite_cast<TParseContext>(
        mut context: TParseContext,
        scalar_context: &ScalarAnalysisContext<'_>,
        syn: &crate::syn::sqlite::ExprCast,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = crate::ast::Expr::try_from(syn)?;
        let result = syntax.resolve(&mut context, scalar_context);
        Ok((result?, context))
    }
}

impl TypeCastExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.arg.walk(visitor)?;
        Ok(())
    }
}

impl ImplicitCastExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.value.walk(visitor)?;
        Ok(())
    }
}

impl TypeCastExpr {
    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution<'_>,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<Option<SemScalarExpr>, crate::sem::AnalysisError> {
        if self.arg.constraint_node().is_none() {
            return Ok(None);
        }
        let target = if context.dialect() == crate::ast::Dialect::postgres() {
            if let Some(target) =
                crate::context::BaseParseContext::canonicalize_type_ref(context, &self.target_type)
            {
                parameters.require(context, &self.arg, &target, true);
                target
            } else {
                self.target_type.clone()
            }
        } else {
            self.target_type.clone()
        };
        Ok(Some(parameters.operation(
            "CAST",
            std::slice::from_ref(&self.arg),
            &crate::sem::InputTypeConstraint::exact(&target),
            None,
        )))
    }
}
