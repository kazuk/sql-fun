mod query;

use crate::sem::unresolved::{
    Access, AlterSequence, BooleanExpr, BooleanOperator, Call, CallKind, Case, CaseArm, Cast,
    Collate, Comment, Delete, DropTable, EnumType, Expr, Extension, Indirection, Insert, Literal,
    Name, NamedArgument, NullTest, OperatorExpr, OperatorKind, Order, Parameter, Query, Sequence,
    Slice, SqlValue, Statement, Subquery, SubqueryKind, SyntaxOption, TruthTest,
    TryCollectFromWithTokens, TryFromWithTokens, TryFromWithTokensOpt, TypeName, Update,
    VariableSet, View, ViewCheckOption, Window,
};
use crate::syn::{ListOpt, Opt};
use crate::{StringSpan, sem::AnalysisError, syn};

use ::sql_fun_core::IVec;

pub(in crate::sem::unresolved) fn required<T>(
    value: Option<T>,
    field: &str,
) -> Result<T, AnalysisError> {
    match value {
        Some(value) => Ok(value),
        None => AnalysisError::raise_unexpected_none(field),
    }
}

fn nonempty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

impl TryFrom<syn::NodeListOpt> for Name {
    type Error = AnalysisError;

    fn try_from(nodes: syn::NodeListOpt) -> Result<Self, Self::Error> {
        let mut parts = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            parts.push(required(node.as_string().get_sval(), "name component")?);
        }
        Ok(Self::from_parts(&parts))
    }
}

impl TryFromWithTokens<&syn::Node> for Statement {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        if let Some(query) = value.as_select_stmt().as_inner() {
            return Ok(Self::query(Query::try_from_with_tokens(&query, tokens)?));
        }
        if let Some(insert) = value.as_insert_stmt().as_inner() {
            return Ok(Self::insert(Insert::try_from_with_tokens(&insert, tokens)?));
        }
        if let Some(update) = value.as_update_stmt().as_inner() {
            return Ok(Self::update(Update::try_from_with_tokens(&update, tokens)?));
        }
        if let Some(delete) = value.as_delete_stmt().as_inner() {
            return Ok(Self::delete(Delete::try_from_with_tokens(&delete, tokens)?));
        }
        if let Some(view) = value.as_view_stmt().as_inner() {
            return Ok(Self::view(View::try_from_with_tokens(&view, tokens)?));
        }
        if let Some(extension) = value.as_create_extension_stmt().as_inner() {
            return Ok(Self::extension(Extension::from(&extension)));
        }
        if let Some(variable_set) = value.as_variable_set_stmt().as_inner() {
            return Ok(Self::variable_set(VariableSet::try_from_with_tokens(
                &variable_set,
                tokens,
            )?));
        }
        if let Some(enum_type) = value.as_create_enum_stmt().as_inner() {
            return Ok(Self::enum_type(EnumType::try_from(&enum_type)?));
        }
        if let Some(sequence) = value.as_create_seq_stmt().as_inner() {
            return Ok(Self::sequence(Sequence::try_from_with_tokens(
                &sequence, tokens,
            )?));
        }
        if let Some(drop_table) = value.as_drop_stmt().as_inner() {
            return Ok(Self::drop_table(DropTable::try_from(&drop_table)?));
        }
        if let Some(alter_sequence) = value.as_alter_seq_stmt().as_inner() {
            return Ok(Self::alter_sequence(AlterSequence::try_from_with_tokens(
                &alter_sequence,
                tokens,
            )?));
        }
        if let Some(comment) = value.as_comment_stmt().as_inner() {
            return Ok(Self::comment(Comment::try_from(&comment)?));
        }
        AnalysisError::raise_unexpected_input("top-level PostgreSQL statement")
    }
}

impl TryFrom<&syn::CommentStmt> for Comment {
    type Error = AnalysisError;

    fn try_from(value: &syn::CommentStmt) -> Result<Self, Self::Error> {
        let object_kind = required(value.get_objtype().as_inner(), "comment_stmt.objtype")?;
        let object = required(value.get_object().as_inner(), "comment_stmt.object")?;
        let target =
            if let Some(items) = object.as_list().get_items().map(|items| items.as_string()) {
                items.iter().map(|item| item.get_sval()).collect()
            } else if let Some(item) = object.as_string().get_sval() {
                vec![item]
            } else {
                return AnalysisError::raise_unexpected_input("comment statement object");
            };
        Ok(Self::new(
            target,
            crate::sem::ObjectKind::from(object_kind as u32),
            &value.get_comment(),
        ))
    }
}

impl TryFromWithTokens<&syn::AlterSeqStmt> for AlterSequence {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::AlterSeqStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let sequence = required(value.get_sequence().as_inner(), "alter_seq.sequence")?;
        let mut parts = Vec::new();
        if let Some(catalog) = nonempty(sequence.get_catalogname()) {
            parts.push(catalog);
        }
        if let Some(schema) = nonempty(sequence.get_schemaname()) {
            parts.push(schema);
        }
        parts.push(sequence.get_relname());
        Ok(Self::new(
            Name::from_parts(&parts),
            SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
            value.get_for_identity(),
            value.get_missing_ok(),
        ))
    }
}

impl TryFrom<&syn::DropStmt> for DropTable {
    type Error = AnalysisError;

    fn try_from(value: &syn::DropStmt) -> Result<Self, Self::Error> {
        if !matches!(
            value.get_remove_type().as_inner(),
            Some(syn::ObjectType::ObjectTable)
        ) {
            return AnalysisError::raise_unexpected_input("drop statement object type");
        }
        let objects = required(value.get_objects().as_inner(), "drop_stmt.objects")?;
        let names = objects
            .iter()
            .map(|object| Name::try_from(object.as_list().get_items()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::new(
            names,
            value.get_missing_ok(),
            matches!(
                value.get_behavior().as_inner(),
                Some(syn::DropBehavior::DropCascade)
            ),
            value.get_concurrent(),
        ))
    }
}

impl TryFromWithTokens<&syn::CreateSeqStmt> for Sequence {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::CreateSeqStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let sequence = required(value.get_sequence().as_inner(), "create_seq.sequence")?;
        let mut parts = Vec::new();
        if let Some(catalog) = nonempty(sequence.get_catalogname()) {
            parts.push(catalog);
        }
        if let Some(schema) = nonempty(sequence.get_schemaname()) {
            parts.push(schema);
        }
        parts.push(sequence.get_relname());
        Ok(Self::new(
            Name::from_parts(&parts),
            SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
            value.get_owner_id(),
            value.get_for_identity(),
            value.get_if_not_exists(),
        ))
    }
}

impl TryFrom<&syn::CreateEnumStmt> for EnumType {
    type Error = AnalysisError;

    fn try_from(value: &syn::CreateEnumStmt) -> Result<Self, Self::Error> {
        let Some(values) = value.get_vals().map(|values| values.as_string()) else {
            return AnalysisError::raise_unexpected_none("create_enum_stmt.vals");
        };
        Ok(Self::new(
            Name::try_from(value.get_type_name())?,
            values.iter().map(|value| value.get_sval()).collect(),
        ))
    }
}

impl TryFromWithTokens<&syn::VariableSetStmt> for VariableSet {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::VariableSetStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::new(
            &value.get_name(),
            Expr::try_collect_from_with_tokens(value.get_args(), tokens)?,
        ))
    }
}

impl From<&syn::CreateExtensionStmt> for Extension {
    fn from(value: &syn::CreateExtensionStmt) -> Self {
        Self::new(&value.get_extname())
    }
}

impl TryFromWithTokens<&syn::ViewStmt> for View {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::ViewStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let view = required(value.get_view().as_inner(), "view_stmt.view")?;
        let query = required(
            value.get_query().as_select_stmt().as_inner(),
            "view_stmt.query",
        )?;
        let mut parts = Vec::new();
        if let Some(catalog) = nonempty(view.get_catalogname()) {
            parts.push(catalog);
        }
        if let Some(schema) = nonempty(view.get_schemaname()) {
            parts.push(schema);
        }
        parts.push(view.get_relname());
        let mut aliases = Vec::new();
        for alias in value.get_aliases().as_inner().unwrap_or_default() {
            aliases.push(required(alias.as_string().get_sval(), "view alias")?);
        }
        Ok(Self::new(
            Name::from_parts(&parts),
            aliases,
            Query::try_from_with_tokens(&query, tokens)?,
            value.get_replace(),
            SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
            ViewCheckOption::from(value.get_with_check_option().as_inner()),
            false,
            false,
            false,
        ))
    }
}

impl From<Option<syn::ViewCheckOption>> for ViewCheckOption {
    fn from(value: Option<syn::ViewCheckOption>) -> Self {
        match value {
            Some(syn::ViewCheckOption::LocalCheckOption) => Self::Local,
            Some(syn::ViewCheckOption::CascadedCheckOption) => Self::Cascaded,
            Some(syn::ViewCheckOption::Undefined)
            | Some(syn::ViewCheckOption::NoCheckOption)
            | Some(syn::ViewCheckOption::OptionNone)
            | None => Self::None,
        }
    }
}

impl Name {
    #[deprecated = "use try_from"]
    pub(in crate::sem::unresolved) fn postgres(
        nodes: syn::NodeListOpt,
    ) -> Result<Self, AnalysisError> {
        let mut parts = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            parts.push(required(node.as_string().get_sval(), "name component")?);
        }
        Ok(Self::from_parts(&parts))
    }
}

impl TryFromWithTokens<&syn::ColumnRef> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        column: &syn::ColumnRef,
        _tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut parts = Vec::new();
        let mut star = false;
        for field in column.get_fields().as_inner().unwrap_or_default() {
            if field.is_a_star() {
                star = true;
            } else {
                parts.push(required(field.as_string().get_sval(), "column name")?);
            }
        }
        let name = Name::from_parts(&parts);
        Ok(if star {
            Self::Star(name)
        } else {
            Self::Column(name)
        })
    }
}

impl TryFromWithTokens<&syn::AExpr> for OperatorExpr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::AExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            name: Name::try_from(value.get_name())?,
            kind: OperatorKind::try_from(required(value.get_kind().as_inner(), "operator kind")?)?,
            left: Expr::try_from_with_tokens_opt(value.get_lexpr(), tokens)?,
            right: Expr::try_from_with_tokens_opt(value.get_rexpr(), tokens)?,
            escape: None,
        })
    }
}

impl TryFromWithTokens<&syn::AExpr> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::AExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Operator(Box::new(
            OperatorExpr::try_from_with_tokens(value, tokens)?,
        )))
    }
}

impl TryFromWithTokens<&syn::BoolExpr> for BooleanExpr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::BoolExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            operator: BooleanOperator::try_from(required(
                value.get_boolop().as_inner(),
                "boolean operator",
            )?)?,
            arguments: Expr::try_collect_from_with_tokens(value.get_args(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::BoolExpr> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::BoolExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Boolean(Box::new(BooleanExpr::try_from_with_tokens(
            value, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::NullTest> for NullTest {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        test: &syn::NullTest,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from_with_tokens(&test.get_arg(), tokens)?,
            negated: matches!(
                test.get_nulltesttype().as_inner(),
                Some(syn::NullTestType::IsNotNull)
            ),
            row: test.get_argisrow(),
        })
    }
}

impl TryFromWithTokens<&syn::NullTest> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        test: &syn::NullTest,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::NullTest(Box::new(NullTest::try_from_with_tokens(
            test, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::BooleanTest> for TruthTest {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        test: &syn::BooleanTest,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let (value, negated) = match required(test.get_booltesttype().as_inner(), "truth test")? {
            syn::BoolTestType::IsTrue => (Some(true), false),
            syn::BoolTestType::IsNotTrue => (Some(true), true),
            syn::BoolTestType::IsFalse => (Some(false), false),
            syn::BoolTestType::IsNotFalse => (Some(false), true),
            syn::BoolTestType::IsUnknown => (None, false),
            syn::BoolTestType::IsNotUnknown => (None, true),
            _ => return AnalysisError::raise_unexpected_input("truth test"),
        };
        Ok(Self {
            argument: Expr::try_from_with_tokens(&test.get_arg(), tokens)?,
            value,
            negated,
        })
    }
}

impl TryFromWithTokens<&syn::BooleanTest> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        test: &syn::BooleanTest,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::TruthTest(Box::new(TruthTest::try_from_with_tokens(
            test, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::TypeCast> for Cast {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        cast: &syn::TypeCast,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from_with_tokens(&cast.get_arg(), tokens)?,
            target: TypeName::try_from_with_tokens(
                &required(cast.get_type_name().as_inner(), "cast type")?,
                tokens,
            )?,
        })
    }
}

impl TryFromWithTokens<&syn::TypeCast> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        cast: &syn::TypeCast,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Cast(Box::new(Cast::try_from_with_tokens(
            cast, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::CollateClause> for Collate {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        collate: &syn::CollateClause,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from_with_tokens(&collate.get_arg(), tokens)?,
            name: Name::try_from(collate.get_collname())?,
        })
    }
}

impl TryFromWithTokens<&syn::CollateClause> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        collate: &syn::CollateClause,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Collate(Box::new(Collate::try_from_with_tokens(
            collate, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::SubLink> for Subquery {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        link: &syn::SubLink,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            query: Query::try_from_with_tokens(
                &required(link.get_subselect().as_select_stmt().as_inner(), "subquery")?,
                tokens,
            )?,
            kind: SubqueryKind::try_from(required(
                link.get_sub_link_type().as_inner(),
                "subquery kind",
            )?)?,
            test: Expr::try_from_with_tokens_opt(link.get_testexpr(), tokens)?,
            operator: Name::try_from(link.get_oper_name())?,
        })
    }
}

impl TryFromWithTokens<&syn::SubLink> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        link: &syn::SubLink,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Subquery(Box::new(Subquery::try_from_with_tokens(
            link, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::AArrayExpr> for Vec<Expr> {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::AArrayExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Expr::try_collect_from_with_tokens(value.get_elements(), tokens)
    }
}

impl TryFromWithTokens<&syn::AArrayExpr> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        array: &syn::AArrayExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Array(Vec::<Self>::try_from_with_tokens(
            array, tokens,
        )?))
    }
}

impl TryFromWithTokens<&syn::AIndirection> for Indirection {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        indirection: &syn::AIndirection,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from_with_tokens(&indirection.get_arg(), tokens)?,
            items: Access::try_collect_from_with_tokens(indirection.get_indirection(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::AIndirection> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        indirection: &syn::AIndirection,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Indirection(Box::new(
            Indirection::try_from_with_tokens(indirection, tokens)?,
        )))
    }
}

impl TryFromWithTokens<&syn::NamedArgExpr> for NamedArgument {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        argument: &syn::NamedArgExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            name: argument.get_name(),
            argument: Expr::try_from_with_tokens(&argument.get_arg(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::NamedArgExpr> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        argument: &syn::NamedArgExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::NamedArgument(Box::new(
            NamedArgument::try_from_with_tokens(argument, tokens)?,
        )))
    }
}

impl TryFromWithTokens<&syn::ParamRef> for Parameter {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::ParamRef,
        _tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            position: value.get_number() as u32,
            name: None,
            type_name: None,
        })
    }
}

impl TryFromWithTokens<&syn::ParamRef> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::ParamRef,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Parameter(Parameter::try_from_with_tokens(
            value, tokens,
        )?))
    }
}

impl TryFromWithTokens<&syn::SqlValueFunction> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::SqlValueFunction,
        _tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::SqlValue(SqlValue::try_from(value)?))
    }
}

impl TryFromWithTokens<&syn::MinMaxExpr> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        minmax: &syn::MinMaxExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let name = match minmax.get_op().as_inner() {
            Some(syn::MinMaxOp::IsGreatest) => "greatest",
            Some(syn::MinMaxOp::IsLeast) => "least",
            _ => return AnalysisError::raise_unexpected_input("min/max operation"),
        };
        Ok(Self::Call(Box::new(Call::special(
            name,
            Self::try_collect_from_with_tokens(minmax.get_args(), tokens)?,
        ))))
    }
}

impl TryFromWithTokens<&syn::GroupingFunc> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        grouping: &syn::GroupingFunc,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Grouping(Self::try_collect_from_with_tokens(
            grouping.get_args(),
            tokens,
        )?))
    }
}

impl TryFromWithTokens<&syn::FuncCall> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::FuncCall,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Call(Box::new(Call::try_from_with_tokens(
            value, tokens,
        )?)))
    }
}

impl TryFromWithTokens<&syn::Node> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        node: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        if node.is_a_const() {
            return Ok(Self::Literal(Literal::from(&node.as_a_const())));
        }
        if let Some(column) = node.as_column_ref().as_inner() {
            return Self::try_from_with_tokens(&column, tokens);
        }
        if let Some(call) = node.as_func_call().as_inner() {
            return Ok(Self::Call(Box::new(Call::try_from_with_tokens(
                &call, tokens,
            )?)));
        }
        if let Some(operator) = node.as_a_expr().as_inner() {
            return Self::try_from_with_tokens(&operator, tokens);
        }
        if let Some(boolean) = node.as_bool_expr().as_inner() {
            return Self::try_from_with_tokens(&boolean, tokens);
        }
        if let Some(test) = node.as_null_test().as_inner() {
            return Self::try_from_with_tokens(&test, tokens);
        }
        if let Some(test) = node.as_boolean_test().as_inner() {
            return Self::try_from_with_tokens(&test, tokens);
        }
        if let Some(cast) = node.as_type_cast().as_inner() {
            return Self::try_from_with_tokens(&cast, tokens);
        }
        if let Some(collate) = node.as_collate_clause().as_inner() {
            return Self::try_from_with_tokens(&collate, tokens);
        }
        if let Some(case) = node.as_case_expr().as_inner() {
            return Self::try_from_with_tokens(&case, tokens);
        }
        if let Some(link) = node.as_sub_link().as_inner() {
            return Self::try_from_with_tokens(&link, tokens);
        }
        if let Some(array) = node.as_a_array_expr().as_inner() {
            return Self::try_from_with_tokens(&array, tokens);
        }
        if let Some(row) = node.as_row_expr().as_inner() {
            return Ok(Self::Row(Self::try_collect_from_with_tokens(
                row.get_args(),
                tokens,
            )?));
        }
        if let Some(list) = node.as_list().as_inner() {
            return Ok(Self::List(Self::try_collect_from_with_tokens(
                list.get_items(),
                tokens,
            )?));
        }
        if let Some(indirection) = node.as_a_indirection().as_inner() {
            return Self::try_from_with_tokens(&indirection, tokens);
        }
        if let Some(argument) = node.as_named_arg_expr().as_inner() {
            return Self::try_from_with_tokens(&argument, tokens);
        }
        if let Some(parameter) = node.as_param_ref().as_inner() {
            return Self::try_from_with_tokens(&parameter, tokens);
        }
        if let Some(value) = node.as_sqlvalue_function().as_inner() {
            return Self::try_from_with_tokens(&value, tokens);
        }
        if let Some(coalesce) = node.as_coalesce_expr().as_inner() {
            return Self::try_from_with_tokens(&coalesce, tokens);
        }
        if let Some(minmax) = node.as_min_max_expr().as_inner() {
            return Self::try_from_with_tokens(&minmax, tokens);
        }
        if node.is_set_to_default() {
            return Ok(Self::Default);
        }
        if let Some(cursor) = node.as_current_of_expr().as_inner() {
            return Ok(Self::CurrentOf(super::CurrentOf {
                name: cursor.get_cursor_name(),
                parameter: cursor.get_cursor_param(),
            }));
        }
        if node.is_merge_support_func() {
            return Ok(Self::MergeAction);
        }
        if let Some(document) =
            super::document::DocumentExpr::try_from_with_tokens_opt(node, tokens)?
        {
            return Ok(Self::Document(document));
        }
        if let Some(grouping) = node.as_grouping_func().as_inner() {
            return Self::try_from_with_tokens(&grouping, tokens);
        }
        if let Some(value) = Option::<Literal>::from(node) {
            return Ok(Self::Literal(value));
        }
        AnalysisError::raise_unexpected_input(&format!("expression syntax: {node:?}"))
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for Expr {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        nodes
            .as_inner()
            .unwrap_or_default()
            .into_iter()
            .map(|node| Self::try_from_with_tokens(&node, tokens))
            .collect()
    }
}

impl TryFromWithTokensOpt<syn::Node> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens_opt(
        node: syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Option<Self>, Self::Error> {
        node.as_inner()
            .map(|node| Self::try_from_with_tokens(&node, tokens))
            .transpose()
    }
}

impl Expr {
    #[deprecated = "use try_from_with_tokens"]
    pub(crate) fn postgres_function(
        value: &syn::FuncCall,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::Call(Box::new(Call::try_from_with_tokens(
            value, tokens,
        )?)))
    }

    #[deprecated = "use try_from_with_tokens"]
    pub(crate) fn postgres(
        node: &syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Self::try_from_with_tokens(node, tokens)
    }

    #[deprecated = "use try_from_with_tokens_opt"]
    pub(in crate::sem::unresolved) fn postgres_optional(
        node: syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Option<Self>, AnalysisError> {
        Self::try_from_with_tokens_opt(node, tokens)
    }

    #[deprecated = "use try_collect_from_with_token"]
    pub(in crate::sem::unresolved) fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        Self::try_collect_from_with_tokens(nodes, tokens)
    }
}

impl From<&syn::AConst> for Literal {
    fn from(value: &syn::AConst) -> Self {
        if let Some(value) = value.as_ival().get_ival() {
            Self::Integer(value)
        } else if let Some(value) = value.as_fval().get_fval() {
            Self::Numeric(value)
        } else if let Some(value) = value.as_boolval().get_boolval() {
            Self::Boolean(value)
        } else if let Some(value) = value.as_sval().get_sval() {
            Self::String(value)
        } else if let Some(value) = value.as_bsval().get_bsval() {
            Self::BitString(value)
        } else {
            Self::Null
        }
    }
}

impl Literal {
    #[deprecated = "use from"]
    pub(in crate::sem::unresolved) fn postgres(value: &syn::AConst) -> Self {
        Self::from(value)
    }
}

impl TryFrom<syn::AExprKind> for OperatorKind {
    type Error = AnalysisError;

    fn try_from(kind: syn::AExprKind) -> Result<Self, Self::Error> {
        Ok(match kind {
            syn::AExprKind::AexprOp => Self::Ordinary,
            syn::AExprKind::AexprOpAny => Self::Any,
            syn::AExprKind::AexprOpAll => Self::All,
            syn::AExprKind::AexprDistinct => Self::Distinct,
            syn::AExprKind::AexprNotDistinct => Self::NotDistinct,
            syn::AExprKind::AexprNullif => Self::NullIf,
            syn::AExprKind::AexprIn => Self::In,
            syn::AExprKind::AexprLike => Self::Like,
            syn::AExprKind::AexprIlike => Self::ILike,
            syn::AExprKind::AexprSimilar => Self::Similar,
            syn::AExprKind::AexprBetween => Self::Between,
            syn::AExprKind::AexprNotBetween => Self::NotBetween,
            syn::AExprKind::AexprBetweenSym => Self::BetweenSymmetric,
            syn::AExprKind::AexprNotBetweenSym => Self::NotBetweenSymmetric,
            _ => return AnalysisError::raise_unexpected_input("operator kind"),
        })
    }
}

impl OperatorKind {
    #[deprecated = "use try_from"]
    pub(in crate::sem::unresolved) fn postgres(
        kind: syn::AExprKind,
    ) -> Result<Self, AnalysisError> {
        Self::try_from(kind)
    }
}

impl TryFrom<syn::BoolExprType> for BooleanOperator {
    type Error = AnalysisError;

    fn try_from(kind: syn::BoolExprType) -> Result<Self, Self::Error> {
        match kind {
            syn::BoolExprType::AndExpr => Ok(Self::And),
            syn::BoolExprType::OrExpr => Ok(Self::Or),
            syn::BoolExprType::NotExpr => Ok(Self::Not),
            _ => AnalysisError::raise_unexpected_input("boolean operator"),
        }
    }
}

impl BooleanOperator {
    #[deprecated = "use try_from"]
    pub(in crate::sem::unresolved) fn postgres(
        kind: syn::BoolExprType,
    ) -> Result<Self, AnalysisError> {
        Self::try_from(kind)
    }
}

impl TryFrom<syn::SubLinkType> for SubqueryKind {
    type Error = AnalysisError;

    fn try_from(value: syn::SubLinkType) -> Result<Self, Self::Error> {
        match value {
            syn::SubLinkType::ExistsSublink => Ok(Self::Exists),
            syn::SubLinkType::AllSublink => Ok(Self::All),
            syn::SubLinkType::AnySublink => Ok(Self::Any),
            syn::SubLinkType::RowcompareSublink => Ok(Self::Row),
            syn::SubLinkType::ExprSublink => Ok(Self::Scalar),
            syn::SubLinkType::MultiexprSublink => Ok(Self::MultiExpression),
            syn::SubLinkType::ArraySublink => Ok(Self::Array),
            _ => AnalysisError::raise_unexpected_input("subquery kind"),
        }
    }
}

impl SubqueryKind {
    #[deprecated = "use try_from"]
    pub(in crate::sem::unresolved) fn postgres(
        kind: syn::SubLinkType,
    ) -> Result<Self, AnalysisError> {
        Self::try_from(kind)
    }
}

impl TryFromWithTokens<&syn::TypeName> for TypeName {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::TypeName,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut bounds = Vec::new();
        for bound in value.get_array_bounds().as_inner().unwrap_or_default() {
            let bound = required(bound.as_integer().get_ival(), "array bound")?;
            bounds.push(if bound < 0 { None } else { Some(bound) });
        }
        Ok(Self {
            name: Name::try_from(value.get_names())?,
            modifiers: Expr::try_collect_from_with_tokens(value.get_typmods(), tokens)?,
            array_bounds: bounds,
            set_of: value.get_setof(),
            percent_type: value.get_pct_type(),
            location: source_span(value.get_location(), tokens),
        })
    }
}

impl TypeName {
    #[deprecated = "use try_from_with_tokens"]
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::TypeName,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Self::try_from_with_tokens(value, tokens)
    }
}

fn source_span(location: i32, tokens: &IVec<syn::ScanToken>) -> Option<StringSpan> {
    if location < 0 {
        return None;
    }
    let mut span = None;
    let mut expect_name = true;
    for token in tokens {
        if token.get_start() < location
            || matches!(
                token.get_token().as_inner(),
                Some(syn::Token::SqlComment | syn::Token::CComment)
            )
        {
            continue;
        }
        if expect_name && token.is_ident() || !expect_name && token.is_dot() {
            let next = StringSpan::new(
                token.get_start() as usize,
                (token.get_end() - token.get_start()) as usize,
            );
            if let Some(span) = &mut span {
                StringSpan::extend(span, &next);
            } else {
                span = Some(next);
            }
            expect_name = !expect_name;
        } else {
            break;
        }
    }
    span
}

#[cfg(test)]
mod test_source_span {
    use crate::PostgresDialect;
    use crate::sem::unresolved::postgres::source_span;

    #[test]
    fn preserves_quoted_qualified_name_and_intervening_comment() -> testresult::TestResult {
        let sql = "select \"schema name\" /* source */ . \"function name\"(1)";
        let tokens = PostgresDialect::scan(sql)?;
        let span = source_span(7, &tokens).expect("function name span");
        assert_eq!(
            span.get_from(sql),
            Some("\"schema name\" /* source */ . \"function name\""),
        );
        assert!(source_span(-1, &tokens).is_none());
        Ok(())
    }
}

impl TryFromWithTokens<&syn::FuncCall> for Call {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::FuncCall,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            name: Name::try_from(value.get_funcname())?,
            kind: CallKind::ordinary(),
            within_order: Vec::new(),
            arguments: Expr::try_collect_from_with_tokens(value.get_args(), tokens)?,
            star: value.get_agg_star(),
            distinct: value.get_agg_distinct(),
            variadic: value.get_func_variadic(),
            within_group: value.get_agg_within_group(),
            order: Order::try_collect_from_with_tokens(value.get_agg_order(), tokens)?,
            filter: Expr::try_from_with_tokens_opt(value.get_agg_filter(), tokens)?,
            over: value
                .get_over()
                .as_inner()
                .map(|window| Window::try_from_with_tokens(&window, tokens))
                .transpose()?,
            location: source_span(value.get_location(), tokens),
        })
    }
}

impl Call {
    fn special(name: &str, arguments: Vec<Expr>) -> Self {
        Self {
            kind: CallKind::special(name),
            name: Name::local(name),
            arguments,
            star: false,
            distinct: false,
            variadic: false,
            within_group: false,
            order: Vec::new(),
            filter: None,
            over: None,
            location: None,
            within_order: Vec::new(),
        }
    }

    #[deprecated = "use try_from_with_tokens"]
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::FuncCall,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Self::try_from_with_tokens(value, tokens)
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for Access {
    type Error = AnalysisError;
    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut items = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            items.push(if let Some(indices) = node.as_a_indices().as_inner() {
                if indices.get_is_slice() {
                    Self::Slice(Slice {
                        lower: Expr::try_from_with_tokens_opt(indices.get_lidx(), tokens)?,
                        upper: Expr::try_from_with_tokens_opt(indices.get_uidx(), tokens)?,
                    })
                } else {
                    Self::Index(Expr::try_from_with_tokens(&indices.get_uidx(), tokens)?)
                }
            } else if node.is_a_star() {
                Self::Star
            } else {
                Self::Field(required(node.as_string().get_sval(), "field name")?)
            });
        }
        Ok(items)
    }
}

impl Access {
    #[deprecated = "use try_collect_from_with_tokens"]
    pub(in crate::sem::unresolved) fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        Self::try_collect_from_with_tokens(nodes, tokens)
    }
}

impl TryFrom<&syn::SqlValueFunction> for SqlValue {
    type Error = AnalysisError;

    fn try_from(value: &syn::SqlValueFunction) -> Result<Self, Self::Error> {
        let (name, precision) = match required(value.get_op().as_inner(), "SQL value")? {
            syn::SqlValueFunctionOp::SvfopCurrentDate => ("current_date", false),
            syn::SqlValueFunctionOp::SvfopCurrentTime => ("current_time", false),
            syn::SqlValueFunctionOp::SvfopCurrentTimeN => ("current_time", true),
            syn::SqlValueFunctionOp::SvfopCurrentTimestamp => ("current_timestamp", false),
            syn::SqlValueFunctionOp::SvfopCurrentTimestampN => ("current_timestamp", true),
            syn::SqlValueFunctionOp::SvfopLocaltime => ("localtime", false),
            syn::SqlValueFunctionOp::SvfopLocaltimeN => ("localtime", true),
            syn::SqlValueFunctionOp::SvfopLocaltimestamp => ("localtimestamp", false),
            syn::SqlValueFunctionOp::SvfopLocaltimestampN => ("localtimestamp", true),
            syn::SqlValueFunctionOp::SvfopCurrentRole => ("current_role", false),
            syn::SqlValueFunctionOp::SvfopCurrentUser => ("current_user", false),
            syn::SqlValueFunctionOp::SvfopUser => ("user", false),
            syn::SqlValueFunctionOp::SvfopSessionUser => ("session_user", false),
            syn::SqlValueFunctionOp::SvfopCurrentCatalog => ("current_catalog", false),
            syn::SqlValueFunctionOp::SvfopCurrentSchema => ("current_schema", false),
            _ => return AnalysisError::raise_unexpected_input("SQL value"),
        };
        Ok(Self {
            name: name.to_owned(),
            precision: if precision {
                Some(value.get_typmod())
            } else {
                None
            },
        })
    }
}

impl SqlValue {
    #[deprecated = "use SqlValue::try_from"]
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::SqlValueFunction,
    ) -> Result<Self, AnalysisError> {
        Self::try_from(value)
    }
}

impl From<&syn::Node> for Option<Literal> {
    fn from(node: &syn::Node) -> Self {
        if let Some(value) = node.as_string().get_sval() {
            Some(Literal::String(value))
        } else if let Some(value) = node.as_integer().get_ival() {
            Some(Literal::Integer(value))
        } else if let Some(value) = node.as_float().get_fval() {
            Some(Literal::Numeric(value))
        } else {
            node.as_boolean().get_boolval().map(Literal::Boolean)
        }
    }
}

impl Literal {
    #[deprecated = "use Option::<Literal>::from"]
    fn postgres_option(node: &syn::Node) -> Option<Self> {
        Option::<Self>::from(node)
    }
}

impl TryFromWithTokens<&syn::CaseExpr> for Expr {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        case: &syn::CaseExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut arms = Vec::new();
        for arm in case.get_args().as_inner().unwrap_or_default() {
            let arm = required(arm.as_case_when().as_inner(), "case arm")?;
            arms.push(CaseArm {
                condition: Self::try_from_with_tokens(&arm.get_expr(), tokens)?,
                result: Self::try_from_with_tokens(&arm.get_result(), tokens)?,
            });
        }
        Ok(Self::Case(Box::new(Case {
            argument: Self::try_from_with_tokens_opt(case.get_arg(), tokens)?,
            arms,
            otherwise: Self::try_from_with_tokens_opt(case.get_defresult(), tokens)?,
        })))
    }
}

impl Expr {
    #[deprecated = "use try_from_with_tokens"]
    pub(crate) fn postgres_case(
        case: &syn::CaseExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Self::try_from_with_tokens(case, tokens)
    }
}

impl TryFromWithTokens<&syn::CoalesceExpr> for Expr {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::CoalesceExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self::Call(Box::new(Call::special(
            "coalesce",
            Self::try_collect_from_with_tokens(value.get_args(), tokens)?,
        ))))
    }
}

impl Expr {
    #[deprecated = "use try_from_with_tokens"]
    pub(crate) fn postgres_coalesce(
        value: &syn::CoalesceExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Self::try_from_with_tokens(value, tokens)
    }
}
