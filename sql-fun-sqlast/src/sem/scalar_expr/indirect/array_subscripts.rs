use crate::{
    ast::{Access, Expr, Slice},
    sem::{
        AnalysisError, ParseContext, ScalarAnalysisContext, SemScalarExpr, TypeReference,
        parameter_resolution::ParameterResolution,
        scalar_expr::indirect::{IndirectionExpr, array_index::ArrayIndex},
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// One source-written dimension in a consecutive PostgreSQL subscript group.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArraySubscript {
    lower: Option<SemScalarExpr>,
    upper: Option<SemScalarExpr>,
    slice: bool,
}

impl ArraySubscript {
    pub(crate) fn index(index: &SemScalarExpr) -> Self {
        Self {
            lower: None,
            upper: Some(index.clone()),
            slice: false,
        }
    }

    pub(crate) fn slice(lower: Option<&SemScalarExpr>, upper: Option<&SemScalarExpr>) -> Self {
        Self {
            lower: lower.cloned(),
            upper: upper.cloned(),
            slice: true,
        }
    }

    /// Whether this dimension explicitly uses slice syntax.
    pub const fn is_slice(&self) -> bool {
        self.slice
    }

    /// Explicit lower bound. In a slice group, a non-slice dimension starts at 1;
    /// an omitted slice bound instead uses the array's corresponding bound.
    pub const fn lower(&self) -> Option<&SemScalarExpr> {
        self.lower.as_ref()
    }

    /// Explicit upper bound, or the index expression for a non-slice dimension.
    /// An omitted slice bound uses the array's corresponding bound.
    pub const fn upper(&self) -> Option<&SemScalarExpr> {
        self.upper.as_ref()
    }

    fn append_bounds<'a>(&'a self, bounds: &mut Vec<&'a SemScalarExpr>) {
        bounds.extend(self.lower.as_ref());
        bounds.extend(self.upper.as_ref());
    }

    fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
        assignment: bool,
    ) -> Result<(), AnalysisError> {
        for bound in [&mut self.lower, &mut self.upper].into_iter().flatten() {
            bound.walk(visitor)?;
            if assignment {
                visitor.subscript(bound);
            }
        }
        Ok(())
    }
    fn visit<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
        assignment: bool,
    ) -> Result<(), AnalysisError> {
        for bound in [&self.lower, &self.upper].into_iter().flatten() {
            bound.visit(visitor)?;
            if assignment {
                visitor.subscript(bound);
            }
        }
        Ok(())
    }
}

/// One array operation containing multiple consecutive PostgreSQL subscripts.
/// Any explicit slice makes the whole operation a slice; nested parenthesized
/// accesses remain separate operations on the base expression.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArraySubscripts {
    array: SemScalarExpr,
    subscripts: Vec<ArraySubscript>,
    result_type: Option<TypeReference>,
}

impl ArraySubscripts {
    pub(super) fn resolve(
        context: &mut impl ParseContext,
        array: &SemScalarExpr,
        subscripts: &[ArraySubscript],
    ) -> Result<Self, AnalysisError> {
        let mut value = Self {
            array: array.clone(),
            subscripts: subscripts.to_vec(),
            result_type: None,
        };
        array.require_array_resolved(context)?;
        for bound in value.bounds() {
            bound.fits_array_index_resolved(context)?;
        }
        value.result_type = if value.is_slice() {
            array.get_type()
        } else if let Some(ty) = array.get_type() {
            ArrayIndex::resolved_item_type(context, &ty)
        } else {
            None
        };
        Ok(value)
    }

    /// Base array expression, including any separate inner access operation.
    pub const fn array(&self) -> &SemScalarExpr {
        &self.array
    }

    /// Source dimensions in their original order, with omitted bounds preserved.
    pub fn subscripts(&self) -> &[ArraySubscript] {
        &self.subscripts
    }

    /// Whether any dimension makes this entire group an array-valued slice.
    pub fn is_slice(&self) -> bool {
        self.subscripts.iter().any(ArraySubscript::is_slice)
    }

    /// Inferred scalar element type or array slice type, if known.
    pub const fn result_type(&self) -> Option<&TypeReference> {
        self.result_type.as_ref()
    }

    pub(super) fn set_result_type(&mut self, result: &Option<TypeReference>) {
        self.result_type = result.clone();
    }

    fn bounds(&self) -> Vec<&SemScalarExpr> {
        let mut bounds = Vec::new();
        for subscript in &self.subscripts {
            subscript.append_bounds(&mut bounds);
        }
        bounds
    }

    pub(super) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.array.walk(visitor)?;
        for subscript in &mut self.subscripts {
            subscript.walk(visitor, false)?;
        }
        Ok(())
    }
    pub(super) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.array.visit(visitor)?;
        for subscript in &self.subscripts {
            subscript.visit(visitor, false)?;
        }
        Ok(())
    }

    pub(super) fn walk_assignment_target<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.array.walk_assignment_target(visitor)?;
        for subscript in &mut self.subscripts {
            subscript.walk(visitor, true)?;
        }
        Ok(())
    }
    pub(super) fn visit_assignment_target<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.array.visit_assignment_target(visitor)?;
        for subscript in &self.subscripts {
            subscript.visit(visitor, true)?;
        }
        Ok(())
    }

    pub(super) fn collect_constraints(
        &self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> SemScalarExpr {
        let mut result =
            parameters.array_access(context, &self.array, &self.bounds(), self.is_slice());
        result.retain_indirection(&IndirectionExpr::subscripts(self));
        result
    }
}

impl ArraySubscript {
    pub(crate) fn from_ast_access(
        ast: &Access,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        match ast {
            Access::Index(index) => Self::from_ast_index(index, context, scope),
            Access::Slice(slice) => Self::from_ast_slice(slice, context, scope),
            _ => AnalysisError::raise_unexpected_input("array subscript"),
        }
    }
}

impl ArraySubscript {
    fn from_ast_index(
        index: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let index = SemScalarExpr::from_ast_in(index, context, scope)?;
        Ok(Self::index(&index))
    }

    pub(crate) fn from_ast_slice(
        slice: &Slice,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let lower = SemScalarExpr::from_ast_optional(&slice.lower, context, scope)?;
        let upper = SemScalarExpr::from_ast_optional(&slice.upper, context, scope)?;
        Ok(Self::slice(lower.as_ref(), upper.as_ref()))
    }
}
