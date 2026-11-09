mod array_index;
mod array_slice;
mod array_subscripts;
mod member_ref;

pub use self::array_subscripts::{ArraySubscript, ArraySubscripts};
use self::{array_index::ArrayIndex, array_slice::ArraySlice, member_ref::MemberRef};
use crate::{
    ast::{Access, Dialect, Expr, Slice},
    context::ParseContext,
    sem::{
        AnalysisError, ColumnDefinition, ColumnRename, ParseContext as SemParseContext,
        ScalarAnalysisContext, SemScalarExpr, TypeReference,
        parameter_resolution::ParameterResolution,
        scalar_expr::{RowExpr, SemScalarExprNode},
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// Indirect expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IndirectionExpr {
    /// Array element index access expression
    ArrayIndex(Box<ArrayIndex>),
    /// Array range slice
    ArraySlice(Box<ArraySlice>),
    /// One consecutive PostgreSQL group of multiple array subscripts.
    ArraySubscripts(Box<ArraySubscripts>),
    /// composite type member
    MemberRef(Box<MemberRef>),
}

impl IndirectionExpr {
    pub(crate) fn projected_member(
        base: &SemScalarExpr,
        field: &ColumnDefinition,
    ) -> SemScalarExpr {
        let name = field.name().as_ref().expect("composite field name");
        SemScalarExpr::indirection(&Self::MemberRef(Box::new(MemberRef::new(
            base.clone(),
            name.as_str().to_owned(),
            field.get_type_cloned(),
            field.is_not_null(),
        ))))
    }

    pub(super) fn row_constructor(&self) -> Option<&RowExpr> {
        if let Self::MemberRef(value) = self {
            value.row_constructor()
        } else {
            None
        }
    }

    pub(super) fn projection_name(&self) -> &str {
        if let Self::MemberRef(value) = self {
            value.row_projection_name()
        } else {
            ""
        }
    }

    pub(super) fn rename_column(&mut self, rename: &ColumnRename) {
        if let Self::MemberRef(value) = self {
            value.rename_column(rename);
        }
    }

    pub(super) fn subscripts(value: &ArraySubscripts) -> Self {
        Self::ArraySubscripts(Box::new(value.clone()))
    }

    /// Consecutive subscripts, when this operation contains multiple dimensions.
    pub fn as_array_subscripts(&self) -> Option<&ArraySubscripts> {
        match self {
            Self::ArraySubscripts(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn resolved_subscripts(
        context: &mut impl ParseContext,
        base: &SemScalarExpr,
        subscripts: &[ArraySubscript],
    ) -> Result<SemScalarExpr, AnalysisError> {
        let group = ArraySubscripts::resolve(context, base, subscripts)?;
        Ok(SemScalarExpr::indirection(&Self::subscripts(&group)))
    }
    pub(super) fn index(value: &ArrayIndex) -> Self {
        Self::ArrayIndex(Box::new(value.clone()))
    }
    pub(super) fn slice(value: &ArraySlice) -> Self {
        Self::ArraySlice(Box::new(value.clone()))
    }
    pub(crate) fn set_result_type(&mut self, result: &Option<TypeReference>) {
        match self {
            Self::ArrayIndex(value) => value.set_item_type(result),
            Self::ArraySlice(_) => {}
            Self::ArraySubscripts(value) => value.set_result_type(result),
            Self::MemberRef(value) => value.set_member_type(result),
        }
    }
    pub(crate) fn resolved_index<TContext: ParseContext>(
        context: &mut TContext,
        base: SemScalarExpr,
        index: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Ok(SemScalarExpr::indirection(&ArrayIndex::index_resolved(
            base, index, context,
        )?))
    }

    pub(crate) fn resolved_slice<TContext: ParseContext>(
        context: &mut TContext,
        base: SemScalarExpr,
        lower: Option<SemScalarExpr>,
        upper: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Ok(SemScalarExpr::indirection(&ArraySlice::slice_resolved(
            base, lower, upper, context,
        )?))
    }

    pub(crate) fn resolved_member<TContext: ParseContext>(
        context: &mut TContext,
        base: SemScalarExpr,
        name: &str,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Ok(SemScalarExpr::indirection(&MemberRef::member_resolved(
            base,
            name.to_owned(),
            context,
        )?))
    }
}

impl SemScalarExprNode for IndirectionExpr {
    fn get_type(&self) -> Option<TypeReference> {
        match &self {
            Self::ArrayIndex(ind) => ind.item_type().clone(),
            Self::ArraySlice(slice) => slice.array_type().clone(),
            Self::ArraySubscripts(value) => value.result_type().cloned(),
            Self::MemberRef(mem_ref) => mem_ref.member_type().clone(),
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        match &self {
            // A non-null container can hold NULL or have no element at the index.
            Self::ArrayIndex(_) => Some(false),
            Self::ArraySlice(slice) => slice.array_is_not_null(),
            Self::ArraySubscripts(value) => {
                if value.is_slice() {
                    value.array().is_not_null()
                } else {
                    Some(false)
                }
            }
            Self::MemberRef(mem_ref) => mem_ref.base_is_not_null(),
        }
    }
}

impl IndirectionExpr {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::ArrayIndex(v) => v.walk(visitor)?,
            Self::ArraySlice(v) => v.walk(visitor)?,
            Self::ArraySubscripts(value) => value.walk(visitor)?,
            Self::MemberRef(v) => v.walk(visitor)?,
        }
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        match self {
            Self::ArrayIndex(v) => v.visit(visitor)?,
            Self::ArraySlice(v) => v.visit(visitor)?,
            Self::ArraySubscripts(value) => value.visit(visitor)?,
            Self::MemberRef(v) => v.visit(visitor)?,
        }
        Ok(())
    }
}

impl IndirectionExpr {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl SemParseContext,
    ) -> Option<SemScalarExpr> {
        match self {
            Self::ArrayIndex(value) => Some(value.collect_constraints(parameters, context)),
            Self::ArraySlice(value) => Some(value.collect_constraints(parameters, context)),
            Self::ArraySubscripts(value) => Some(value.collect_constraints(parameters, context)),
            Self::MemberRef(value) => value.collect_constraints(parameters),
        }
    }
}

impl IndirectionExpr {
    pub(crate) fn walk_assignment_target<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::ArrayIndex(target) => target.walk_assignment_target(visitor),
            Self::ArraySlice(target) => target.walk_assignment_target(visitor),
            Self::ArraySubscripts(value) => value.walk_assignment_target(visitor),
            Self::MemberRef(target) => target.walk_assignment_target(visitor),
        }
    }
    pub(crate) fn visit_assignment_target<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::ArrayIndex(target) => target.visit_assignment_target(visitor),
            Self::ArraySlice(target) => target.visit_assignment_target(visitor),
            Self::ArraySubscripts(value) => value.visit_assignment_target(visitor),
            Self::MemberRef(target) => target.visit_assignment_target(visitor),
        }
    }
}

impl IndirectionExpr {
    pub(crate) fn from_ast_sequence(
        items: &[Access],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        mut base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut remaining = items;
        while let Some((first, rest)) = remaining.split_first() {
            let count = if context.dialect() == Dialect::postgres() {
                Self::ast_subscript_prefix_length(remaining)
            } else {
                0
            };
            if count > 1 {
                let (group, rest) = remaining.split_at(count);
                let mut subscripts = Vec::with_capacity(count);
                for access in group {
                    subscripts.push(ArraySubscript::from_ast_access(access, context, scope)?);
                }
                base = Self::resolved_subscripts(context, &base, &subscripts)?;
                remaining = rest;
            } else {
                base = Self::from_ast_access(first, context, scope, base)?;
                remaining = rest;
            }
        }
        Ok(base)
    }
}

impl IndirectionExpr {
    pub(crate) fn ast_subscript_prefix_length(items: &[Access]) -> usize {
        let mut count = 0;
        for access in items {
            if !Self::ast_is_subscript(access) {
                break;
            }
            count += 1;
        }
        count
    }
}

impl IndirectionExpr {
    pub(crate) const fn ast_is_subscript(ast: &Access) -> bool {
        matches!(ast, Access::Index(_) | Access::Slice(_))
    }
}

impl IndirectionExpr {
    pub(crate) fn from_ast_access(
        ast: &Access,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match ast {
            Access::Field(name) => Self::resolved_member(context, base, name),
            Access::Index(index) => Self::from_ast_index(index, context, scope, base),
            Access::Slice(slice) => Self::from_ast_slice(slice, context, scope, base),
            Access::Star => SemScalarExpr::ast_invalid_star_projection(context),
        }
    }
}

impl IndirectionExpr {
    fn from_ast_index(
        index: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let index = SemScalarExpr::from_ast_in(index, context, scope)?;
        Self::resolved_index(context, base, index)
    }

    fn from_ast_slice(
        slice: &Slice,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let bounds = ArraySubscript::from_ast_slice(slice, context, scope)?;
        Self::resolved_slice(
            context,
            base,
            bounds.lower().cloned(),
            bounds.upper().cloned(),
        )
    }
}

impl IndirectionExpr {
    fn selected_row_field(&self) -> Option<&SemScalarExpr> {
        let Self::MemberRef(member) = self else {
            return None;
        };
        let row = member.base().as_whole_row()?;
        row.relation()?;
        row.field_value(member.member())
    }
}
impl SemScalarExpr {
    /// Bound exposed field selected from a nominal whole-row source.
    pub(crate) fn selected_row_field(&self) -> Option<&Self> {
        self.as_indirection()?.selected_row_field()
    }
}
