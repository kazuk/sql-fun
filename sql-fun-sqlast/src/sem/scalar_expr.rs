mod analyze;
mod arith_expr;
mod array;
mod array_subquery;
mod boolean;
mod case_expr;
mod cast_expr;
mod coalesce;
mod collate;
mod column_ref;
mod const_expr;
mod constrained;
mod context;
mod distinct;
mod exists;
mod func_call;
pub(crate) mod grouping;
mod indirect;
mod membership;
mod min_max;
mod named_arg;
mod param_ref;
mod resolved;
mod source_column;
mod sql_value;
mod sub_link;
mod truth_test;
mod whole_row;

pub use self::{
    arith_expr::ArithExpr,
    array::ArrayExpr,
    array_subquery::ArraySubqueryExpr,
    boolean::{BooleanExpr, NullTestExpr},
    case_expr::{CaseExpr, CaseWhen},
    cast_expr::{ImplicitCastExpr, TypeCastExpr},
    coalesce::CoalesceExpr,
    collate::CollateExpr,
    column_ref::{
        ColumnReferenceExpr, CteColumnRef, MergeRecordColumnRef, OutputColumnRef,
        QualifiedColumnFailure, SubQueryColumnRef, SystemColumnRef, TableColumnRef,
    },
    const_expr::ScalarConstExpr,
    constrained::ConstrainedExpr,
    context::ScalarAnalysisContext,
    distinct::DistinctExpr,
    exists::ExistsExpr,
    func_call::{FuncCallArgs, FuncCallExpr, FunctionBinding},
    grouping::{GroupingArgument, GroupingExpr},
    indirect::{ArraySubscript, ArraySubscripts, IndirectionExpr},
    membership::{MembershipComparison, MembershipExpr},
    min_max::MinMaxExpr,
    named_arg::NamedArgExpr,
    param_ref::ParamRef,
    source_column::SourceColumnExpr,
    sql_value::SqlValueExpr,
    sub_link::SubLinkExpr,
    truth_test::TruthTestExpr,
    whole_row::WholeRowExpr,
};
pub(crate) use self::{case_expr::CaseExprAnalysisContext, whole_row::WholeRowFields};

use crate::sem::{
    AnalysisError, AnalysisProblem, ColumnName, ParseContext, PgBuiltInType, TypeReference,
    create_table::ColumnDefinition,
};

trait SemScalarExprNode {
    fn get_type(&self) -> Option<TypeReference>;
    fn is_not_null(&self) -> Option<bool>;
}

/// Internal argument container; it has no scalar type, nullability or output name.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ListExpr(Vec<SemScalarExpr>);

/// An anonymous SQL row with ordered, analyzed fields.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RowExpr(Vec<SemScalarExpr>);

impl RowExpr {
    pub(crate) fn new(fields: &[SemScalarExpr]) -> Self {
        Self(fields.to_vec())
    }

    /// Fields in SQL constructor order.
    pub fn fields(&self) -> &[SemScalarExpr] {
        &self.0
    }

    pub(super) fn field(&self, name: &str) -> Option<&SemScalarExpr> {
        let index = name
            .strip_prefix('f')?
            .parse::<usize>()
            .ok()?
            .checked_sub(1)?;
        if name != format!("f{}", index + 1) {
            return None;
        }
        self.0.get(index)
    }

    fn coerced_for_cast(
        &self,
        context: &impl ParseContext,
        target: &TypeReference,
    ) -> Option<Self> {
        let targets = target.row_columns(context)?.field_types()?;
        if self.0.len() != targets.len() {
            return None;
        }
        let fields: Option<Vec<_>> = self
            .0
            .iter()
            .zip(targets)
            .map(|(value, target)| value.coerced_row_field(context, &target))
            .collect();
        Some(Self::new(&fields?))
    }

    fn get_type(&self) -> TypeReference {
        let columns = self
            .0
            .iter()
            .enumerate()
            .map(|(index, value)| {
                ColumnDefinition::new(
                    &Some(ColumnName::from(format!("f{}", index + 1).as_str())),
                    value.get_type().as_ref(),
                    value.is_not_null(),
                    &[],
                )
            })
            .collect();
        TypeReference::dynamic(&crate::sem::TypeDefinition::ImplicitComposite(
            crate::sem::ColumnCollection::new(columns),
        ))
    }

    fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for field in &mut self.0 {
            field.walk(visitor)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod row_cast_tests {
    use super::{ScalarConstExpr, SemScalarExpr};
    use crate::sem::{
        CastContext, CastDefinition, ColumnCollection, ColumnDefinition, PgBuiltInType,
        TypeDefinition, TypeReference,
    };
    use crate::test_helpers::TestParseContext;

    #[test]
    fn field_casts_retain_target_types_and_nullability() {
        let mut context = TestParseContext::default();
        context.set_get_implicit_cast_result(
            &PgBuiltInType::int4(),
            &PgBuiltInType::int8(),
            Some(CastDefinition::new(CastContext::explicit())),
        );
        let target = TypeReference::dynamic(&TypeDefinition::implicit_composite(
            &ColumnCollection::new(vec![
                ColumnDefinition::new(&None, Some(&PgBuiltInType::int8()), None, &[]),
                ColumnDefinition::new(&None, Some(&PgBuiltInType::int4()), None, &[]),
            ]),
        ));
        let mut value = SemScalarExpr::row(&[
            SemScalarExpr::literal(&ScalarConstExpr::new_integer(1)),
            ScalarConstExpr::null(),
        ]);
        assert!(value.coerce_row_cast(&context, &target));
        let fields = value.as_row().unwrap().fields();
        assert_eq!(fields[0].get_type(), Some(PgBuiltInType::int8()));
        assert_eq!(fields[0].is_not_null(), Some(true));
        assert_eq!(fields[1].get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(fields[1].is_not_null(), Some(false));
    }

    #[test]
    fn rejected_row_cast_preserves_original_fields() {
        let context = TestParseContext::default();
        let mut value =
            SemScalarExpr::row(&[SemScalarExpr::literal(&ScalarConstExpr::new_integer(1))]);
        let original = value.clone();
        let target = TypeReference::dynamic(&TypeDefinition::implicit_composite(
            &ColumnCollection::new(vec![ColumnDefinition::new(
                &None,
                Some(&PgBuiltInType::bool()),
                None,
                &[],
            )]),
        ));
        assert!(!value.coerce_row_cast(&context, &target));
        assert_eq!(value, original);
    }
}

impl ListExpr {
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, index: usize) -> Option<&SemScalarExpr> {
        self.0.get(index)
    }

    pub fn into_inner(self) -> Vec<SemScalarExpr> {
        self.0
    }
}

#[cfg(test)]
mod list_contract_tests {
    use super::{ScalarConstExpr, SemScalarExpr};

    #[test]
    fn internal_lists_have_no_scalar_metadata_and_retain_their_arguments() {
        let element = SemScalarExpr::literal(&ScalarConstExpr::new_integer(1));
        let list = SemScalarExpr::list(std::slice::from_ref(&element));
        assert!(!list.is_membership_row());
        assert_eq!(list.get_type(), None);
        assert_eq!(list.is_not_null(), None);
        assert_eq!(list.get_column_def(), None);
        assert_eq!(list.get_column_name(), "");
        assert_eq!(list.into_list_values().unwrap(), vec![element]);
    }
}

/// scalar explation in semantic ast
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SemScalarExpr {
    /// Bound expression-backed FROM column, retaining its value and null extension.
    SourceColumn(Box<SourceColumnExpr>),
    /// A qualified whole-row value with its completed source lookup.
    WholeRow(Box<WholeRowExpr>),
    /// Anonymous SQL row constructor.
    Row(RowExpr),
    /// array expression
    Array(ArrayExpr),
    /// ARRAY(query) retaining its analyzed child and result type domain.
    ArraySubquery(ArraySubqueryExpr),
    /// const expression
    Const(ScalarConstExpr),
    /// column reference expression
    ColumnRef(ColumnReferenceExpr),
    /// function call expression
    FuncCall(FuncCallExpr),
    /// PostgreSQL grouping bit mask with resolved grouping-key bindings.
    Grouping(GroupingExpr),
    /// arith expression
    Arith(ArithExpr),
    /// type casting expression
    TypeCast(TypeCastExpr),
    /// cast expression
    Case(CaseExpr),
    /// boolean expression
    Boolean(BooleanExpr),
    /// null test expression
    NullTest(NullTestExpr),
    /// SQL truth predicate retaining its operand and tested truth value.
    TruthTest(TruthTestExpr),
    /// Null-safe distinctness, retaining its analyzed comparison.
    Distinct(DistinctExpr),
    /// IN/NOT IN with analyzed operands and comparison bindings.
    Membership(Box<MembershipExpr>),
    /// sub-query expression
    SubLink(SubLinkExpr),
    /// EXISTS predicate retaining its analyzed query.
    Exists(ExistsExpr),
    /// calesce expression
    Coalesce(CoalesceExpr),
    /// min-max expressions
    MinMax(MinMaxExpr),
    /// collate explressions
    Collate(CollateExpr),
    /// SQL standard value expression
    SqlValue(SqlValueExpr),
    /// parameter reference expression
    Param(ParamRef),

    /// SQL constraint expression retaining parameter-dependent type alternatives.
    Constrained(ConstrainedExpr),
    /// named argument expression
    NamedArg(NamedArgExpr),
    /// indirectrion expression
    Indirection(IndirectionExpr),
    /// implicit cast expression
    ///
    /// # Note
    ///
    /// `ImplicitCast` is not in Syntax level AST.
    ///
    /// `sql-fun` inserts implicit cast at a implicit conversion as needed.
    ///
    ImplicitCast(ImplicitCastExpr),

    /// expression list
    List(ListExpr),

    /// The action performed by the enclosing PostgreSQL MERGE.
    MergeAction,

    /// Unexpected syntax node in expression
    Unexpected(String),
}

impl SemScalarExpr {
    pub(crate) fn matches_column(&self, rename: &crate::sem::ColumnRename) -> bool {
        if let Self::ColumnRef(value) = self {
            value.matches_column(rename)
        } else {
            false
        }
    }

    pub(crate) const fn namespace_reference(
        &self,
    ) -> Option<(&str, &crate::sem::ColumnName, bool)> {
        if let Self::ColumnRef(value) = self {
            value.namespace_reference()
        } else {
            None
        }
    }

    pub(crate) const fn mark_unqualified_column(mut self) -> Self {
        if let Self::ColumnRef(value) = &mut self {
            value.mark_unqualified();
        }
        self
    }
    pub(crate) fn renamed_column_output(
        &self,
        rename: &crate::sem::ColumnRename,
    ) -> Option<&crate::sem::ColumnName> {
        if let Self::ColumnRef(value) = self {
            value.renamed_output(rename)
        } else {
            None
        }
    }

    pub(crate) fn set_projection_name(&mut self, name: &crate::sem::ColumnName) {
        if let Self::ColumnRef(value) = self {
            value.set_projection_name(name);
        }
    }
    pub(crate) fn rename_bound_column(&mut self, rename: &crate::sem::ColumnRename) {
        match self {
            Self::ColumnRef(value) => value.rename_column(rename),
            Self::WholeRow(value) => value.rename_column(rename),
            Self::Indirection(value) => value.rename_column(rename),
            _ => {}
        }
    }

    pub(crate) fn rename_bound_relation(
        &mut self,
        old: &crate::sem::TableName,
        new: &crate::sem::TableName,
    ) {
        match self {
            Self::ColumnRef(value) => value.rename_relation(old, new),
            Self::WholeRow(value) => value.rename_relation(old, new),
            Self::FuncCall(value) => value.rename_relation(old, new),
            Self::TypeCast(value) => value.rename_relation(old, new),
            Self::ImplicitCast(value) => value.rename_relation(old, new),
            Self::Membership(value) => value.rename_relation(old, new),
            _ => {}
        }
    }

    pub(super) fn grouping_references_resolved(&self) -> bool {
        match self {
            Self::ColumnRef(value) => value.grouping_reference_resolved(),
            Self::Grouping(value) => value.is_valid(),
            Self::FuncCall(value) => value.name_resolution().is_resolved(),
            Self::TypeCast(value) => value.resolution().is_resolved(),
            Self::Arith(value) => value
                .binding()
                .is_none_or(|binding| binding.resolution().is_resolved()),
            Self::Unexpected(_) => false,
            _ => true,
        }
    }
    pub(super) fn normalize_grouping_cast(&mut self) {
        if let Self::TypeCast(value) = self
            && let Some(operand) = value.grouping_key_operand()
        {
            *self = operand.clone();
        }
    }
    pub(super) fn canonicalize_grouping_key(&mut self) {
        match self {
            Self::Grouping(value) => value.canonicalize_grouping_key(),
            Self::MinMax(value) => value.canonicalize_grouping_key(),
            Self::Array(value) => value.canonicalize_grouping_key(),
            Self::ArraySubquery(value) => value.canonicalize_grouping_key(),
            Self::Case(value) => value.canonicalize_grouping_key(),
            Self::TruthTest(value) => value.canonicalize_grouping_key(),
            Self::Distinct(value) => value.canonicalize_grouping_key(),
            Self::Membership(value) => value.canonicalize_grouping_key(),
            Self::Coalesce(value) => value.canonicalize_grouping_key(),
            Self::FuncCall(value) => value.canonicalize_grouping_key(),
            Self::Arith(value) => value.canonicalize_grouping_key(),
            Self::Param(value) => value.canonicalize_grouping_key(),
            Self::NamedArg(value) => value.canonicalize_grouping_key(),
            Self::Collate(value) => value.canonicalize_grouping_key(),
            Self::Constrained(value) => value.canonicalize_grouping_key(),
            Self::ColumnRef(value) => value.canonicalize_grouping_key(),
            _ => {}
        }
    }
    /// Completed qualification failure for a direct column reference.
    pub const fn qualification_failure(&self) -> Option<&self::column_ref::QualifiedColumnFailure> {
        if let Self::ColumnRef(reference) = self {
            reference.qualification_failure()
        } else {
            None
        }
    }
    /// Resolved table-column binding, when this expression directly references one.
    pub const fn table_column_reference(&self) -> Option<&TableColumnRef> {
        if let Self::ColumnRef(reference) = self {
            reference.table_column_reference()
        } else {
            None
        }
    }
    /// Resolved PostgreSQL system attribute, when directly referenced.
    pub const fn system_column_reference(&self) -> Option<&SystemColumnRef> {
        if let Self::ColumnRef(reference) = self {
            reference.system_column_reference()
        } else {
            None
        }
    }
    pub(crate) const fn function_dependency(&self) -> Option<&crate::sem::FullName> {
        if let Self::FuncCall(call) = self {
            call.name_resolution().function_identity()
        } else {
            None
        }
    }

    /// MERGE record binding, when this is a direct column reference.
    pub const fn merge_record_reference(&self) -> Option<&self::column_ref::MergeRecordColumnRef> {
        match self {
            Self::ColumnRef(reference) => reference.merge_record_reference(),
            _ => None,
        }
    }

    pub(crate) fn coerce_row_cast(
        &mut self,
        context: &impl crate::sem::ParseContext,
        target: &TypeReference,
    ) -> bool {
        let Self::Row(row) = self else {
            return false;
        };
        let record = TypeReference::from_master_type_name("record");
        if *target == record {
            return true;
        }
        let Some(coerced) = row.coerced_for_cast(context, target) else {
            // Generic record I/O casts (for example to text) use the existing cast service.
            return context.get_explicit_cast(&record, target).is_some();
        };
        *row = coerced;
        true
    }

    fn coerced_row_field(
        &self,
        context: &impl ParseContext,
        target: &TypeReference,
    ) -> Option<Self> {
        let mut value = self.clone();
        let row_cast = value.coerce_row_cast(context, target);
        if let Some(source) = value.get_type() {
            if source == *target {
                return Some(value);
            }
            let convertible = if source.is_dynamic() {
                source.implicit_match_context(target, context).is_some()
            } else {
                context.get_explicit_cast(&source, target).is_some()
            };
            if !row_cast && !convertible {
                return None;
            }
        }
        Some(Self::explicit_cast(&value, target))
    }

    const fn is_row_constructor(&self) -> bool {
        matches!(self, Self::Row(_))
    }

    pub(crate) const fn merge_action() -> Self {
        Self::MergeAction
    }
    /// PostgreSQL DELETE RETURNING row-image binding, when this is a direct column.
    pub const fn delete_row_reference(&self) -> Option<&crate::sem::DeleteRowColumn> {
        match self {
            Self::ColumnRef(reference) => reference.delete_row_reference(),
            _ => None,
        }
    }
    pub(crate) fn merge_record_column(
        self,
        role: crate::sem::MergeRecordRole,
        nullable: bool,
    ) -> Self {
        match self {
            Self::ColumnRef(reference) => Self::column_reference(
                ColumnReferenceExpr::merge_record(role, &reference, nullable),
            ),
            _ => unreachable!("MERGE record lookup must produce a column reference"),
        }
    }
    pub(crate) fn delete_row_column(self, version: crate::sem::DeleteRowVersion) -> Self {
        match self {
            Self::ColumnRef(reference) => {
                Self::column_reference(ColumnReferenceExpr::delete_row(version, &reference))
            }
            _ => unreachable!("row-image lookup must produce a column reference"),
        }
    }
    pub(crate) fn bound_view_column(&self, view: &crate::sem::FullName) -> Option<&ColumnName> {
        if let Self::ColumnRef(column) = self {
            column.bound_view_column(view)
        } else {
            None
        }
    }

    pub(crate) fn is_invalid_sampling_function(
        &self,
        context: &impl crate::context::BaseParseContext,
    ) -> bool {
        if let Self::FuncCall(call) = self {
            call.modifiers().over().is_some() || call.returns_set(context) == Some(true)
        } else if let Self::Constrained(value) = self {
            value
                .function_binding()
                .is_some_and(|binding| binding.modifiers().over().is_some())
        } else {
            false
        }
    }

    pub(crate) fn preserves_view_rows(
        &self,
        context: &impl crate::context::BaseParseContext,
    ) -> bool {
        if let Self::FuncCall(call) = self {
            if call.signature_is_deferred() || call.modifiers().over().is_some() {
                return false;
            }
            if let Some(overload) = call.overload()
                && let Some(name) = call.name_resolution().function_identity()
            {
                return context.function_preserves_view_rows(name, overload);
            }
        }
        true
    }

    pub(crate) fn view_source_column(&self) -> Option<&ColumnName> {
        if let Self::ColumnRef(value) = self {
            value.view_source_column()
        } else {
            None
        }
    }

    pub(crate) fn with_source_reference_name(
        self,
        alias: &crate::sem::AliasName,
        column: &crate::sem::ColumnName,
    ) -> Self {
        match self {
            Self::SourceColumn(value) => {
                Self::SourceColumn(Box::new(value.with_reference_name(alias, column)))
            }
            Self::ColumnRef(reference) => {
                Self::column_reference(reference.with_reference_name(alias, column))
            }
            _ => self,
        }
    }

    pub(crate) const fn column_reference(reference: ColumnReferenceExpr) -> Self {
        Self::ColumnRef(reference)
    }
    pub(crate) fn set_cast_resolution(&mut self, resolution: crate::sem::QueryNameResolution) {
        if let Self::TypeCast(cast) = self {
            cast.set_resolution(resolution);
        }
    }
    fn retain_cast_binding(&mut self, binding: &crate::sem::QueryNameResolution) {
        if let Self::Constrained(value) = self {
            value.retain_cast_binding(binding);
        }
    }
    fn retain_operator_binding(&mut self, binding: &crate::sem::ResolvedOperator) {
        if let Self::Constrained(value) = self {
            value.retain_operator_binding(binding);
        }
    }
    pub(crate) fn retain_semantic_call(&mut self, call: &FuncCallExpr) {
        if let Self::Constrained(value) = self {
            value.retain_semantic_call(call);
        }
    }
    /// Completed operator-name binding retained before and after parameter inference.
    pub fn operator_binding(&self) -> Option<&crate::sem::ResolvedOperator> {
        match self {
            Self::Arith(value) => value.binding(),
            Self::Constrained(value) => value.operator_binding(),
            _ => None,
        }
    }
    pub(crate) fn set_operator_binding(&mut self, binding: crate::sem::ResolvedOperator) {
        if let Self::Arith(call) = self {
            call.set_binding(binding);
        }
    }
    pub(crate) fn set_function_candidates(
        &mut self,
        candidates: Option<Vec<crate::sem::OverloadVariant>>,
    ) {
        if let Self::FuncCall(call) = self {
            call.set_candidates(candidates);
        }
    }
    pub(crate) fn set_function_name_resolution(
        &mut self,
        outcome: crate::sem::QueryNameResolution,
    ) {
        if let Self::FuncCall(call) = self {
            call.set_name_resolution(outcome);
        }
    }
    pub(crate) fn set_function_modifiers(&mut self, modifiers: crate::sem::FunctionModifiers) {
        if let Self::FuncCall(call) = self {
            call.set_modifiers(modifiers);
        }
    }
    pub(crate) const fn output_index(&self) -> Option<usize> {
        match self {
            Self::ColumnRef(value) => value.output_index(),
            _ => None,
        }
    }
    pub(crate) fn constraint_binding(&self) -> Option<(&str, &ColumnName)> {
        match self {
            Self::ColumnRef(value) => value.constraint_binding(),
            _ => None,
        }
    }

    pub(crate) const fn constraint_node(&self) -> Option<usize> {
        match self {
            Self::SourceColumn(value) => value.value().constraint_node(),
            Self::Param(value) => Some(value.node()),
            Self::Constrained(value) => Some(value.node()),
            Self::MinMax(value) => value.node(),
            Self::Array(value) => value.node(),
            Self::ArraySubquery(value) => value.node(),
            Self::Case(value) => value.node(),
            Self::TruthTest(value) => value.node(),
            Self::Distinct(value) => value.node(),
            Self::Membership(value) => value.node(),
            Self::Coalesce(value) => value.node(),
            Self::Collate(value) => value.argument().constraint_node(),
            _ => None,
        }
    }
    pub(crate) fn parameter(
        position: u32,
        node: usize,
        constraint: &crate::sem::InputTypeConstraint,
    ) -> Self {
        Self::Param(ParamRef::new(position, node, constraint))
    }
    /// Retained explicit collation reference and its analyzed operand.
    pub const fn as_collate(&self) -> Option<&CollateExpr> {
        if let Self::Collate(value) = self {
            Some(value)
        } else {
            None
        }
    }

    pub(crate) const fn collate(value: CollateExpr) -> Self {
        Self::Collate(value)
    }

    pub(crate) fn validate_collation(
        &mut self,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<(), AnalysisError> {
        if let Self::Collate(value) = self {
            value.validate_type(context)?;
        }
        Ok(())
    }

    /// Structured COALESCE with analyzed operands and inferred coercions.
    pub const fn as_coalesce(&self) -> Option<&CoalesceExpr> {
        if let Self::Coalesce(value) = self {
            Some(value)
        } else {
            None
        }
    }
    /// Structured CASE with analyzed conditions, results, and implicit coercions.
    pub const fn as_case(&self) -> Option<&CaseExpr> {
        match self {
            Self::Case(value) => Some(value),
            _ => None,
        }
    }
    /// Analyzed ARRAY constructor, including its elements and type outcome.
    pub const fn as_array(&self) -> Option<&ArrayExpr> {
        if let Self::Array(value) = self {
            Some(value)
        } else {
            None
        }
    }
    /// PostgreSQL GREATEST/LEAST semantics, when this is a MinMax expression.
    pub const fn as_min_max(&self) -> Option<&MinMaxExpr> {
        if let Self::MinMax(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub(crate) fn min_max(value: &MinMaxExpr) -> Self {
        Self::MinMax(value.clone())
    }
    pub(crate) fn retain_indirection(&mut self, indirection: &IndirectionExpr) {
        if let Self::Constrained(value) = self {
            value.retain_indirection(indirection);
        }
    }
    pub(crate) fn constrained(value: &ConstrainedExpr) -> Self {
        Self::Constrained(value.clone())
    }

    /// get Const variant
    pub const fn as_const(&self) -> Option<&ScalarConstExpr> {
        match self {
            Self::Const(c) => Some(c),
            _ => None,
        }
    }

    /// get SqlValue variant
    pub const fn as_sql_value(&self) -> Option<&SqlValueExpr> {
        match self {
            Self::SqlValue(v) => Some(v),
            _ => None,
        }
    }

    /// create new const expr
    #[cfg(test)]
    pub const fn new_const(value: ScalarConstExpr) -> Self {
        Self::Const(value)
    }

    fn builtin_boolean_type() -> Option<TypeReference> {
        Some(TypeReference::concrete_type_ref(
            PgBuiltInType::bool().full_name(),
            false,
        ))
    }

    /// determine expression value type
    #[must_use]
    pub fn get_type(&self) -> Option<TypeReference> {
        match self {
            Self::SourceColumn(value) => value.value().get_type(),
            Self::Const(scalar_const_expr) => scalar_const_expr.get_type(),
            Self::FuncCall(func_call_expr) => func_call_expr.get_type(),
            Self::Arith(arith_expr) => arith_expr.get_type(),
            Self::TypeCast(type_cast_expr) => type_cast_expr.get_type(),
            Self::Case(case_expr) => case_expr.get_type(),
            Self::Boolean(_boolean_expr) => Self::builtin_boolean_type(),
            Self::NullTest(_null_test_expr) => Self::builtin_boolean_type(),
            Self::TruthTest(value) => value.get_type(),
            Self::Distinct(value) => value.get_type(),
            Self::Membership(value) => value.get_type(),
            Self::SubLink(sub_link_expr) => sub_link_expr.get_type(),
            Self::Exists(value) => value.get_type(),
            Self::ArraySubquery(value) => value.get_type(),
            Self::Coalesce(coalesce_expr) => coalesce_expr.get_type(),
            Self::Row(row) => Some(row.get_type()),
            Self::WholeRow(row) => row.get_type(),
            Self::MinMax(min_max_expr) => min_max_expr.get_type(),
            Self::Collate(collate_expr) => collate_expr.get_type(),
            Self::SqlValue(sql_value_expr) => sql_value_expr.get_type(),
            Self::Constrained(value) => value.single_type(),
            Self::Param(param_ref) => param_ref.get_type(),
            Self::NamedArg(named_arg_expr) => named_arg_expr.get_type(),
            Self::Indirection(ind) => ind.get_type(),
            Self::ImplicitCast(ice) => ice.get_type(),
            Self::Array(arr) => arr.get_type(),
            Self::Grouping(_) => Some(PgBuiltInType::int4()),
            Self::MergeAction => Some(PgBuiltInType::text()),
            Self::Unexpected(_node) => None,
            Self::ColumnRef(cr) => cr.get_type(),
            Self::List(_) => None,
        }
    }

    /// determine expression value is nullable
    #[must_use]
    pub fn is_not_null(&self) -> Option<bool> {
        match self {
            Self::SourceColumn(value) => value.is_not_null(),
            Self::Const(scalar_const_expr) => scalar_const_expr.is_not_null(),
            Self::FuncCall(func_call_expr) => func_call_expr.is_not_null(),
            Self::Arith(arith_expr) => arith_expr.is_not_null(),
            Self::TypeCast(type_cast_expr) => type_cast_expr.is_not_null(),
            Self::Case(case_expr) => case_expr.is_not_null(),
            Self::Boolean(boolean_expr) => boolean_expr.is_not_null(),
            Self::NullTest(null_test_expr) => null_test_expr.is_not_null(),
            Self::TruthTest(value) => value.is_not_null(),
            Self::Distinct(value) => value.is_not_null(),
            Self::Membership(value) => value.is_not_null(),
            Self::SubLink(sub_link_expr) => sub_link_expr.is_not_null(),
            Self::Exists(value) => value.is_not_null(),
            Self::ArraySubquery(value) => value.is_not_null(),
            Self::Coalesce(coalesce_expr) => coalesce_expr.is_not_null(),
            Self::Row(_) => Some(true),
            Self::WholeRow(row) => row.is_not_null(),
            Self::MinMax(min_max_expr) => min_max_expr.is_not_null(),
            Self::Collate(collate_expr) => collate_expr.is_not_null(),
            Self::SqlValue(sql_value_expr) => sql_value_expr.is_not_null(),
            Self::Constrained(value) => value.is_not_null(),
            Self::Param(param_ref) => param_ref.is_not_null(),
            Self::NamedArg(named_arg_expr) => named_arg_expr.is_not_null(),
            Self::Indirection(ind) => ind.is_not_null(),
            Self::ImplicitCast(ice) => ice.is_not_null(),
            Self::Array(arr) => arr.is_not_null(),
            Self::Grouping(_) | Self::MergeAction => Some(true),
            Self::Unexpected(_node) => None,
            Self::ColumnRef(cr) => cr.is_not_null(),
            Self::List(_) => None,
        }
    }
}

#[cfg(test)]
mod test_get_type_and_is_not_null {
    use crate::sem::PgBuiltInType;
    use crate::sem::scalar_expr::{ScalarConstExpr, SemScalarExpr};

    #[test]
    fn get_type_and_is_not_null_for_const() {
        let int_expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(10));
        let null_expr = SemScalarExpr::Const(ScalarConstExpr::Null);

        assert_eq!(int_expr.get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(int_expr.is_not_null(), Some(true));
        assert_eq!(null_expr.get_type(), None);
        assert_eq!(null_expr.is_not_null(), Some(false));
    }
}

impl SemScalarExpr {
    /// get column definition
    #[must_use]
    pub fn get_column_def(&self) -> Option<ColumnDefinition> {
        match self {
            Self::SourceColumn(value) => Some(value.definition()),
            Self::ColumnRef(c) => c.get_column_def().clone(),
            Self::TypeCast(_)
            | Self::FuncCall(_)
            | Self::Param(_)
            | Self::Constrained(_)
            | Self::Exists(_)
            | Self::ArraySubquery(_)
            | Self::Membership(_)
            | Self::Distinct(_)
            | Self::TruthTest(_)
            | Self::Grouping(_)
            | Self::MergeAction
            | Self::WholeRow(_)
            | Self::Row(_)
            | Self::List(_) => None,
            _ => todo!("not implemented {self:?}"),
        }
    }

    pub(super) fn constructor_projection_name(&self) -> Option<&'static str> {
        match self {
            Self::Array(_) | Self::ArraySubquery(_) => Some("array"),
            Self::Row(_) => Some("row"),
            Self::TypeCast(value) => value.constructor_projection_name(),
            _ => None,
        }
    }

    /// get column name
    #[must_use]
    pub fn get_column_name(&self) -> String {
        match self {
            Self::SourceColumn(value) => value.name().as_str().to_owned(),
            Self::Const(_scalar_const_expr) => String::new(),
            Self::ColumnRef(cr) => cr.get_column_name().to_string(),
            Self::FuncCall(call) => call.name().local_name(),
            Self::Arith(_arith_expr) => String::new(),
            Self::TypeCast(value) => value
                .constructor_projection_name()
                .unwrap_or_default()
                .to_owned(),
            Self::Case(_case_expr) => String::new(),
            Self::Boolean(_boolean_expr) => String::new(),
            Self::NullTest(_null_test_expr) => String::new(),
            Self::TruthTest(_) | Self::Distinct(_) | Self::Membership(_) => String::new(),
            Self::SubLink(query) => query.output_name(),
            Self::Exists(_) => String::new(),
            Self::Coalesce(coalesce_expr) => coalesce_expr.join_key_name().unwrap_or("").to_owned(),
            Self::Row(_) => "row".to_owned(),
            Self::WholeRow(row) => row.qualifier().name().as_str().to_owned(),
            Self::MinMax(value) => value.operation().to_owned(),
            Self::Collate(value) => value.argument().get_column_name(),
            Self::SqlValue(_sql_value_expr) => todo!(),
            Self::Param(_) => String::new(),
            Self::Constrained(value) => value.name().to_owned(),
            Self::NamedArg(named_arg_expr) => named_arg_expr.value().get_column_name(),
            Self::Indirection(value) => value.projection_name().to_owned(),
            Self::ImplicitCast(value) => value.projection_operand().0.get_column_name(),
            Self::Array(_) | Self::ArraySubquery(_) => "array".to_owned(),
            Self::Grouping(_) => "grouping".to_owned(),
            Self::MergeAction => "merge_action".to_owned(),
            Self::Unexpected(_node) => todo!(),
            Self::List(_) => String::new(),
        }
    }
}

#[cfg(test)]
mod test_get_column_def_and_name {
    use crate::sem::scalar_expr::{
        ColumnReferenceExpr, SemScalarExpr, TableColumnRef, TypeReference,
    };
    use crate::sem::{
        FullName, PgBuiltInType,
        create_table::{ColumnDefinition, ColumnName, TableName},
        data_source::AliasName,
    };

    fn make_table_column_expr(
        column_name: &str,
        column_type: &TypeReference,
        is_not_null: Option<bool>,
    ) -> SemScalarExpr {
        let alias = AliasName::from("t");
        let column = ColumnName::from(column_name);
        let table_name = TableName::from(FullName::with_schema("public", "tbl"));
        let col_def =
            ColumnDefinition::new(&Some(column.clone()), Some(column_type), is_not_null, &[]);
        let table_column = TableColumnRef::new(&alias, &column, &table_name, Some(&col_def), false);
        SemScalarExpr::ColumnRef(ColumnReferenceExpr::TableColumn(table_column))
    }

    #[test]
    fn get_column_def_and_name_from_column_ref() {
        let col_type = PgBuiltInType::int4();
        let expr = make_table_column_expr("col", &col_type, Some(true));

        let col_def = expr.get_column_def().expect("column definition");
        assert_eq!(col_def.get_type_cloned(), Some(col_type));
        assert_eq!(expr.get_column_name(), "col");
    }
}

impl SemScalarExpr {
    /// convert `value_expr` with implicit casting
    #[must_use]
    pub fn wrap_implicit_cast(target_type: &TypeReference, value_expr: &Self) -> Self {
        Self::ImplicitCast(ImplicitCastExpr::new(value_expr, target_type))
    }
}

#[cfg(test)]
mod test_wrap_implicit_cast {
    use crate::sem::PgBuiltInType;
    use crate::sem::scalar_expr::{ScalarConstExpr, SemScalarExpr};

    #[test]
    fn wrap_implicit_cast_sets_type() {
        let base_expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(1));
        let target_type = PgBuiltInType::int8();
        let wrapped = SemScalarExpr::wrap_implicit_cast(&target_type, &base_expr);

        match wrapped {
            SemScalarExpr::ImplicitCast(_) => {
                assert_eq!(wrapped.get_type(), Some(target_type));
            }
            _ => panic!("expected implicit cast expression"),
        }
    }
}

impl SemScalarExpr {
    /// convert expression type with implicit cast
    pub fn implicit_cast_if_require<TParseContext>(
        context: &mut TParseContext,
        result_type: &TypeReference,
        expr: &mut Self,
    ) -> Result<(), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::implicit_cast_resolved(context, result_type, expr)
    }

    pub(crate) fn implicit_cast_resolved<TContext: crate::sem::ParseContext>(
        context: &mut TContext,
        result_type: &TypeReference,
        expr: &mut Self,
    ) -> Result<(), AnalysisError> {
        let Some(ty) = expr.get_type() else {
            return Ok(());
        };
        if &ty == result_type {
            return Ok(());
        }

        let Some(cast) = context.get_implicit_cast(&ty, result_type) else {
            context.report_analysis_problem(AnalysisProblem::implicit_cast_not_found(
                &ty,
                result_type,
            ))?;
            return Ok(());
        };
        if !cast.is_no_conversion() {
            *expr = Self::wrap_implicit_cast(result_type, expr);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_implicit_cast_if_require {
    use crate::sem::scalar_expr::{ScalarConstExpr, SemScalarExpr};
    use crate::sem::{CastContext, CastDefinition, PgBuiltInType};
    use crate::test_helpers::TestParseContext;

    #[test]
    fn implicit_cast_if_require_no_change_when_same_type() {
        let mut context = TestParseContext::default();
        let target_type = PgBuiltInType::int4();
        let mut expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(3));

        SemScalarExpr::implicit_cast_if_require(&mut context, &target_type, &mut expr)
            .expect("implicit cast check");

        assert!(matches!(expr, SemScalarExpr::Const(_)));
        assert_eq!(context.reported_problem_count(), 0);
    }

    #[test]
    fn implicit_cast_if_require_wraps_on_available_cast() {
        let mut context = TestParseContext::default();
        let source_type = PgBuiltInType::int4();
        let target_type = PgBuiltInType::int8();
        context.set_get_implicit_cast_result(
            &source_type,
            &target_type,
            Some(CastDefinition::new(CastContext::Implicit)),
        );

        let mut expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(3));
        SemScalarExpr::implicit_cast_if_require(&mut context, &target_type, &mut expr)
            .expect("implicit cast check");

        assert!(matches!(expr, SemScalarExpr::ImplicitCast(_)));
        assert_eq!(expr.get_type(), Some(target_type));
        assert_eq!(context.reported_problem_count(), 0);
    }

    #[test]
    fn implicit_cast_if_require_reports_when_missing_cast() {
        let mut context = TestParseContext::default();
        let target_type = PgBuiltInType::int8();
        let mut expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(3));

        SemScalarExpr::implicit_cast_if_require(&mut context, &target_type, &mut expr)
            .expect("implicit cast check");

        assert!(matches!(expr, SemScalarExpr::Const(_)));
        assert_eq!(context.reported_problem_count(), 1);
    }
}

impl SemScalarExpr {
    /// test expression is array and report
    pub fn require_array<TParseContext>(
        &self,
        context: &mut TParseContext,
    ) -> Result<(), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        self.require_array_resolved(context)
    }

    pub(crate) fn require_array_resolved<TContext: crate::sem::ParseContext>(
        &self,
        context: &mut TContext,
    ) -> Result<(), AnalysisError> {
        if let Some(ty) = self.get_type()
            && let Some(is_array) = ty.is_array()
            && !is_array
        {
            context.report_analysis_problem(AnalysisProblem::array_required(self, &ty))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_require_array {
    use crate::sem::PgBuiltInType;
    use crate::sem::scalar_expr::{
        ImplicitCastExpr, ScalarConstExpr, SemScalarExpr, TypeReference,
    };
    use crate::test_helpers::TestParseContext;

    #[test]
    fn require_array_reports_for_non_array() {
        let mut context = TestParseContext::default();
        let expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(5));

        expr.require_array(&mut context).expect("require array");
        assert_eq!(context.reported_problem_count(), 1);
    }

    #[test]
    fn require_array_accepts_array_type() {
        let mut context = TestParseContext::default();
        let base_expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(5));
        let array_type = TypeReference::concrete_type_ref(PgBuiltInType::int4().full_name(), true);
        let expr = SemScalarExpr::ImplicitCast(ImplicitCastExpr::new(&base_expr, &array_type));

        expr.require_array(&mut context).expect("require array");
        assert_eq!(context.reported_problem_count(), 0);
    }
}

impl SemScalarExpr {
    /// test fits array index
    pub fn fits_array_index<TParseContext>(
        &self,
        context: &mut TParseContext,
    ) -> Result<(), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        self.fits_array_index_resolved(context)
    }

    pub(crate) fn fits_array_index_resolved<TContext: crate::sem::ParseContext>(
        &self,
        context: &mut TContext,
    ) -> Result<(), AnalysisError> {
        if let Some(ty) = self.get_type()
            && ty != PgBuiltInType::int4()
            && context
                .get_assignment_cast(&ty, &PgBuiltInType::int4())
                .is_none()
        {
            context
                .report_analysis_problem(AnalysisProblem::array_index_type_mismatch(self, &ty))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_fits_array_index {
    use crate::sem::scalar_expr::{ScalarConstExpr, SemScalarExpr};
    use crate::sem::{CastContext, CastDefinition, PgBuiltInType};
    use crate::test_helpers::TestParseContext;

    #[test]
    fn fits_array_index_reports_when_not_castable() {
        let mut context = TestParseContext::default();
        let expr = SemScalarExpr::Const(ScalarConstExpr::String("value".to_string()));

        expr.fits_array_index(&mut context)
            .expect("array index check");
        assert_eq!(context.reported_problem_count(), 1);
    }

    #[test]
    fn fits_array_index_accepts_castable_type() {
        let mut context = TestParseContext::default();
        let int4 = PgBuiltInType::int4();
        context.set_get_implicit_cast_result(
            &int4,
            &int4,
            Some(CastDefinition::new(CastContext::NoConversion)),
        );
        let expr = SemScalarExpr::Const(ScalarConstExpr::new_integer(7));

        expr.fits_array_index(&mut context)
            .expect("array index check");
        assert_eq!(context.reported_problem_count(), 0);
    }
}

impl ListExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        for expression in &mut self.0 {
            expression.walk(visitor)?;
        }
        Ok(())
    }
}

impl SemScalarExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        crate::sem::with_stack(|| self.walk_body(visitor))?;
        Ok(())
    }
}

impl SemScalarExpr {
    fn walk_body<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        visitor.enter_expression(self)?;
        match self {
            Self::SourceColumn(value) => value.walk(visitor)?,
            Self::Grouping(v) => v.walk(visitor)?,
            Self::Array(v) => v.walk(visitor)?,
            Self::FuncCall(v) => v.walk(visitor)?,
            Self::NamedArg(v) => v.walk(visitor)?,
            Self::Arith(v) => v.walk(visitor)?,
            Self::TypeCast(v) => v.walk(visitor)?,
            Self::Case(v) => v.walk(visitor)?,
            Self::Boolean(v) => v.walk(visitor)?,
            Self::NullTest(v) => v.walk(visitor)?,
            Self::TruthTest(value) => value.walk(visitor)?,
            Self::Distinct(value) => value.walk(visitor)?,
            Self::Membership(value) => value.walk(visitor)?,
            Self::SubLink(v) => v.walk(visitor)?,
            Self::Exists(value) => value.walk(visitor)?,
            Self::ArraySubquery(value) => value.walk(visitor)?,
            Self::Coalesce(v) => v.walk(visitor)?,
            Self::Collate(v) => v.walk(visitor)?,
            Self::Row(v) => v.walk(visitor)?,
            Self::WholeRow(v) => v.walk(visitor)?,
            Self::MinMax(v) => v.walk(visitor)?,
            Self::Constrained(v) => v.walk(visitor)?,
            Self::Indirection(v) => v.walk(visitor)?,
            Self::ImplicitCast(v) => v.walk(visitor)?,
            Self::List(v) => v.walk(visitor)?,
            Self::Const(_)
            | Self::ColumnRef(_)
            | Self::SqlValue(_)
            | Self::Param(_)
            | Self::MergeAction
            | Self::Unexpected(_) => {}
        }
        visitor.expression(self)
    }
}

impl SemScalarExpr {
    /// Preserve a supplied argument name without resolving a formal parameter.
    pub fn named_argument(name: &str, value: &Self, span: &Option<crate::StringSpan>) -> Self {
        Self::NamedArg(NamedArgExpr::new(name, value, span))
    }
    /// Array/composite access, including a parameter-dependent analyzed binding.
    pub fn as_indirection(&self) -> Option<&IndirectionExpr> {
        match self {
            Self::Indirection(value) => Some(value),
            Self::Constrained(value) => value.indirection(),
            _ => None,
        }
    }
    /// Function invocation retained independently of signature resolution.
    pub const fn as_function_call(&self) -> Option<&FuncCallExpr> {
        match self {
            Self::FuncCall(value) => Some(value),
            _ => None,
        }
    }
    /// Supplied named argument, if this expression is an argument wrapper.
    pub const fn as_named_argument(&self) -> Option<&NamedArgExpr> {
        match self {
            Self::NamedArg(value) => Some(value),
            _ => None,
        }
    }
    pub(crate) fn binary_argument(&self, index: usize) -> Option<&Self> {
        match self {
            Self::Arith(value) => value.binary_argument(index),
            Self::Constrained(value) => value.arguments().get(index),
            _ => None,
        }
    }
    pub(crate) fn needs_type_inference(&self) -> bool {
        self.constraint_node().is_some() && self.get_type().is_none()
    }
    pub(crate) const fn is_unconstrained_parameter(&self) -> bool {
        match self {
            Self::Param(value) => value.constraint().is_any(),
            _ => false,
        }
    }
    pub(crate) fn apply_constraints(&mut self, constraints: &crate::sem::ParameterConstraints) {
        match self {
            Self::Param(value) => value.apply_constraints(constraints),
            Self::Constrained(value) => value.apply_constraints(constraints),
            Self::MinMax(value) => value.apply_constraints(constraints),
            Self::Array(value) => value.apply_constraints(constraints),
            Self::ArraySubquery(value) => value.apply_constraints(constraints),
            Self::Case(value) => value.apply_constraints(constraints),
            Self::TruthTest(value) => value.apply_constraints(constraints),
            Self::Distinct(value) => value.apply_constraints(constraints),
            Self::Membership(value) => value.apply_constraints(constraints),
            Self::Coalesce(value) => value.apply_constraints(constraints),
            _ => {}
        }
    }
    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution<'_>,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<(), AnalysisError> {
        let replacement = match self {
            Self::Param(value) => Some(parameters.parameter(value.position(), value.constraint())),
            Self::ColumnRef(_) => parameters.column(self),
            Self::Arith(value) => value.collect_constraints(parameters, context)?,
            Self::FuncCall(value) => value.collect_constraints(parameters, context)?,
            Self::TypeCast(value) => value.collect_constraints(parameters, context)?,
            Self::Boolean(value) => value.collect_constraints(parameters, context),
            Self::NullTest(value) => value.collect_constraints(parameters),
            Self::Membership(value) => {
                value.collect_constraints(parameters, context)?;
                None
            }
            Self::Distinct(value) => {
                value.collect_constraints(parameters, context)?;
                None
            }
            Self::TruthTest(value) => {
                value.collect_constraints(parameters, context);
                None
            }
            Self::SubLink(_) => parameters.scalar_result(self),
            // Child constraints are collected by traversal; EXISTS has its own boolean type.
            Self::Exists(_) => None,
            Self::ArraySubquery(value) => {
                value.collect_constraints(parameters, context)?;
                None
            }
            Self::Case(value) => {
                value.collect_constraints(parameters, context)?;
                None
            }
            Self::Array(value) => {
                value.collect_constraints(parameters, context)?;
                None
            }
            Self::MinMax(value) => {
                value.collect_constraints(parameters, context)?;
                None
            }
            Self::Coalesce(value) => {
                value.collect_constraints(parameters, context)?;
                None
            }
            Self::Indirection(value) => value.collect_constraints(parameters, context),
            _ => None,
        };
        if let Some(mut replacement) = replacement {
            if let Self::TypeCast(cast) = self {
                replacement.retain_cast_binding(cast.resolution());
            }
            if let Self::Arith(operator) = self
                && let Some(binding) = operator.binding()
            {
                replacement.retain_operator_binding(binding);
            }
            *self = replacement;
        }
        Ok(())
    }
}

impl SemScalarExpr {
    /// Ordered analyzed fields of an anonymous SQL row constructor.
    pub const fn as_row(&self) -> Option<&RowExpr> {
        if let Self::Row(row) = self {
            Some(row)
        } else {
            None
        }
    }

    pub(super) fn row_constructor(&self) -> Option<&RowExpr> {
        match self {
            Self::Row(value) => Some(value),
            Self::Indirection(value) => value.row_constructor(),
            Self::TypeCast(value) => value.record_operand()?.row_constructor(),
            Self::Constrained(value) => value.indirection()?.row_constructor(),
            _ => None,
        }
    }

    pub(crate) fn validate_scalar_value(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if matches!(self, Self::List(_))
            || (context.dialect() == crate::ast::Dialect::sqlite() && self.as_row().is_some())
        {
            context.report_analysis_problem(AnalysisProblem::query(
                crate::sem::QueryProblem::scalar_value_required(),
            ))?;
        }
        Ok(())
    }

    /// Membership predicate, including analyzed list or query operands.
    pub fn as_membership(&self) -> Option<&MembershipExpr> {
        match self {
            Self::Membership(value) => Some(value),
            _ => None,
        }
    }
    pub(crate) fn membership(value: MembershipExpr) -> Self {
        Self::Membership(Box::new(value))
    }
    pub(crate) const fn is_membership_row(&self) -> bool {
        matches!(self, Self::Row(_))
    }
    pub(crate) fn membership_fields(&self) -> &[Self] {
        match self {
            Self::Row(value) => &value.0,
            _ => std::slice::from_ref(self),
        }
    }
    pub(crate) fn requires_argument_type(&self) -> bool {
        self.get_type().is_none() && !self.as_const().is_some_and(ScalarConstExpr::is_null)
    }
    pub(crate) const fn arithmetic(value: ArithExpr) -> Self {
        Self::Arith(value)
    }
}

impl SemScalarExpr {
    pub(in crate::sem) fn index_column_name(&self) -> Option<&ColumnName> {
        if let Self::ColumnRef(value) = self {
            value.index_column_name()
        } else {
            None
        }
    }

    pub(in crate::sem) fn canonicalize_index_binding(&mut self) {
        if let Self::ColumnRef(value) = self {
            value.canonicalize_index_binding();
        }
    }

    pub(crate) fn walk_assignment_target<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        if let Self::Indirection(target) = self {
            target.walk_assignment_target(visitor)?;
        }
        visitor.assignment_target(self);
        Ok(())
    }
}

impl SemScalarExpr {
    pub(crate) fn resolve_between(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if let Self::Arith(value) = self
            && let Some(resolved) = value.resolve_between(context)?
        {
            *self = resolved;
        }
        Ok(())
    }
}

impl SemScalarExpr {
    // Unknown literals are SQL input values, not typed text operands. A known
    // peer supplies the comparison target; two literals use string comparison.
    pub(crate) fn comparison_literal_operands(
        context: &impl ParseContext,
        overloads: Option<&[crate::sem::BinaryOperatorDefinition]>,
        left: Self,
        right: Self,
    ) -> (Self, Self) {
        let left_unknown =
            crate::sem::MinMaxExpr::input_type(&left) == Some(crate::sem::MinMaxExpr::unknown());
        let right_unknown =
            crate::sem::MinMaxExpr::input_type(&right) == Some(crate::sem::MinMaxExpr::unknown());
        let left_target = if right_unknown {
            Some(crate::sem::PgBuiltInType::text())
        } else {
            right.get_type()
        };
        let right_target = if left_unknown {
            Some(crate::sem::PgBuiltInType::text())
        } else {
            left.get_type()
        };
        let left = if left_unknown && let Some(ty) = left_target {
            Self::wrap_implicit_cast(
                &Self::comparison_literal_target(context, overloads, &ty),
                &left,
            )
        } else {
            left
        };
        let right = if right_unknown && let Some(ty) = right_target {
            Self::wrap_implicit_cast(
                &Self::comparison_literal_target(context, overloads, &ty),
                &right,
            )
        } else {
            right
        };
        (left, right)
    }

    fn comparison_literal_target(
        context: &impl ParseContext,
        overloads: Option<&[crate::sem::BinaryOperatorDefinition]>,
        known: &TypeReference,
    ) -> TypeReference {
        // PostgreSQL tries (domain, domain) before reducing a domain paired
        // with an unknown literal to its base. Retain the original signature.
        if overloads.is_some_and(|overloads| {
            overloads.iter().any(|candidate| {
                candidate.left_arg_type() == known && candidate.right_arg_type() == known
            })
        }) {
            known.clone()
        } else {
            crate::sem::MinMaxExpr::base_type(context, known)
        }
    }
}

#[cfg(test)]
mod test_domain_comparison_literals {
    use crate::sem::{PgBuiltInType, ScalarConstExpr, SemScalarExpr};
    use crate::test_helpers::domain_casts;

    #[rstest::rstest]
    fn unknown_literal_checks_domain_exact_signature_before_base(
        #[values(false, true)] domain_overload: bool,
        #[values(false, true)] domain_left: bool,
    ) {
        let mut context = domain_casts::context();
        let domain = domain_casts::domain(&mut context, "amount", Some(&PgBuiltInType::int4()));
        let mut operators = domain_casts::operators(&domain);
        if !domain_overload {
            operators.remove(0);
        }
        let known = SemScalarExpr::wrap_implicit_cast(
            &domain,
            &SemScalarExpr::new_const(ScalarConstExpr::new_integer(1)),
        );
        let unknown = SemScalarExpr::new_const(ScalarConstExpr::string("1"));
        let (left, right) = if domain_left {
            (known, unknown)
        } else {
            (unknown, known)
        };
        let (left, right) =
            SemScalarExpr::comparison_literal_operands(&context, Some(&operators), left, right);
        let literal = if domain_left { right } else { left };
        assert_eq!(
            literal.get_type(),
            Some(if domain_overload {
                domain
            } else {
                PgBuiltInType::int4()
            })
        );
    }
}

impl SemScalarExpr {
    pub(crate) fn invalid_from_scalar(
        &self,
        context: &impl crate::context::ParseContext,
        allow_set: bool,
    ) -> bool {
        if let Self::FuncCall(call) = self {
            call.invalid_from(context, allow_set)
        } else {
            false
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn table_root_depth(&self) -> (usize, bool) {
        match self {
            Self::TypeCast(cast) => cast.table_root_depth(),
            Self::FuncCall(_) => (1, false),
            _ => (0, false),
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn explicit_cast_operand(&self) -> Option<&Self> {
        if let Self::TypeCast(cast) = self {
            Some(cast.projection_operand().0)
        } else {
            None
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn placement_result_type(&self) -> Option<TypeReference> {
        match self {
            Self::FuncCall(call) => call.placement_result_type(),
            _ => self.get_type(),
        }
    }
}
