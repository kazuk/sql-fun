use crate::sem::unresolved::document::DocumentExpr;
use crate::sem::unresolved::postgres::required;
use crate::sem::unresolved::{
    Alias, Assignment, CmdType, Expr, FromItem, JsonAggregate, JsonArgument, JsonArray,
    JsonArrayAggregate, JsonArrayQuery, JsonBehavior, JsonBehaviorType, JsonColumn, JsonEncoding,
    JsonExprOp, JsonFormat, JsonFormatType, JsonFunction, JsonKeyValue, JsonObject,
    JsonObjectAggregate, JsonOutput, JsonParse, JsonPath, JsonPredicate, JsonQuotes, JsonReturning,
    JsonScalar, JsonSerialize, JsonTable, JsonTableColumnType, JsonValue, JsonValueType,
    JsonWrapper, Merge, MergeMatchKind, MergeWhen, MultiAssignment, Name, Order, OverridingKind,
    Query, Relation, Target, TypeName, Window, With, Xml, XmlColumn, XmlExprOp, XmlOptionType,
    XmlSerialize, XmlTable,
};
use crate::syn::{ListOpt, Opt};
use crate::{StringSpan, sem::AnalysisError, syn};

use ::sql_fun_core::IVec;

impl JsonExprOp {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::JsonExprOp,
    ) -> Result<Self, AnalysisError> {
        match value {
            syn::JsonExprOp::JsonExistsOp => Ok(Self::Exists),
            syn::JsonExprOp::JsonQueryOp => Ok(Self::Query),
            syn::JsonExprOp::JsonValueOp => Ok(Self::Value),
            syn::JsonExprOp::JsonTableOp => Ok(Self::Table),
            _ => AnalysisError::raise_unexpected_input("JsonExprOp"),
        }
    }
}

impl JsonFormatType {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::JsonFormatType,
    ) -> Result<Self, AnalysisError> {
        match value {
            syn::JsonFormatType::JsFormatDefault => Ok(Self::Default),
            syn::JsonFormatType::JsFormatJson => Ok(Self::Json),
            syn::JsonFormatType::JsFormatJsonb => Ok(Self::Jsonb),
            _ => AnalysisError::raise_unexpected_input("JsonFormatType"),
        }
    }
}

impl JsonEncoding {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::JsonEncoding,
    ) -> Result<Self, AnalysisError> {
        match value {
            syn::JsonEncoding::JsEncDefault => Ok(Self::Default),
            syn::JsonEncoding::JsEncUtf8 => Ok(Self::Utf8),
            syn::JsonEncoding::JsEncUtf16 => Ok(Self::Utf16),
            syn::JsonEncoding::JsEncUtf32 => Ok(Self::Utf32),
            _ => AnalysisError::raise_unexpected_input("JsonEncoding"),
        }
    }
}

impl JsonValueType {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::JsonValueType,
    ) -> Result<Self, AnalysisError> {
        match value {
            syn::JsonValueType::JsTypeAny => Ok(Self::Any),
            syn::JsonValueType::JsTypeObject => Ok(Self::Object),
            syn::JsonValueType::JsTypeArray => Ok(Self::Array),
            syn::JsonValueType::JsTypeScalar => Ok(Self::Scalar),
            _ => AnalysisError::raise_unexpected_input("JsonValueType"),
        }
    }
}

impl JsonBehaviorType {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::JsonBehaviorType,
    ) -> Result<Self, AnalysisError> {
        match value {
            syn::JsonBehaviorType::JsonBehaviorNull => Ok(Self::Null),
            syn::JsonBehaviorType::JsonBehaviorError => Ok(Self::Error),
            syn::JsonBehaviorType::JsonBehaviorEmpty => Ok(Self::Empty),
            syn::JsonBehaviorType::JsonBehaviorTrue => Ok(Self::True),
            syn::JsonBehaviorType::JsonBehaviorFalse => Ok(Self::False),
            syn::JsonBehaviorType::JsonBehaviorUnknown => Ok(Self::Unknown),
            syn::JsonBehaviorType::JsonBehaviorEmptyArray => Ok(Self::EmptyArray),
            syn::JsonBehaviorType::JsonBehaviorEmptyObject => Ok(Self::EmptyObject),
            syn::JsonBehaviorType::JsonBehaviorDefault => Ok(Self::Default),
            _ => AnalysisError::raise_unexpected_input("JsonBehaviorType"),
        }
    }
}

impl JsonWrapper {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::JsonWrapper,
    ) -> Result<Self, AnalysisError> {
        match value {
            syn::JsonWrapper::JswUnspec => Ok(Self::Unspec),
            syn::JsonWrapper::JswNone => Ok(Self::None),
            syn::JsonWrapper::JswConditional => Ok(Self::Conditional),
            syn::JsonWrapper::JswUnconditional => Ok(Self::Unconditional),
            _ => AnalysisError::raise_unexpected_input("JsonWrapper"),
        }
    }
}

impl JsonQuotes {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::JsonQuotes,
    ) -> Result<Self, AnalysisError> {
        match value {
            syn::JsonQuotes::JsQuotesUnspec => Ok(Self::Unspec),
            syn::JsonQuotes::JsQuotesKeep => Ok(Self::Keep),
            syn::JsonQuotes::JsQuotesOmit => Ok(Self::Omit),
            _ => AnalysisError::raise_unexpected_input("JsonQuotes"),
        }
    }
}

impl JsonTableColumnType {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::JsonTableColumnType,
    ) -> Result<Self, AnalysisError> {
        match value {
            syn::JsonTableColumnType::JtcForOrdinality => Ok(Self::ForOrdinality),
            syn::JsonTableColumnType::JtcRegular => Ok(Self::Regular),
            syn::JsonTableColumnType::JtcExists => Ok(Self::Exists),
            syn::JsonTableColumnType::JtcFormatted => Ok(Self::Formatted),
            syn::JsonTableColumnType::JtcNested => Ok(Self::Nested),
            _ => AnalysisError::raise_unexpected_input("JsonTableColumnType"),
        }
    }
}

impl XmlExprOp {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::XmlExprOp,
    ) -> Result<Self, AnalysisError> {
        match value {
            syn::XmlExprOp::IsXmlconcat => Ok(Self::Xmlconcat),
            syn::XmlExprOp::IsXmlelement => Ok(Self::Xmlelement),
            syn::XmlExprOp::IsXmlforest => Ok(Self::Xmlforest),
            syn::XmlExprOp::IsXmlparse => Ok(Self::Xmlparse),
            syn::XmlExprOp::IsXmlpi => Ok(Self::Xmlpi),
            syn::XmlExprOp::IsXmlroot => Ok(Self::Xmlroot),
            syn::XmlExprOp::IsXmlserialize => Ok(Self::Xmlserialize),
            syn::XmlExprOp::IsDocument => Ok(Self::Document),
            _ => AnalysisError::raise_unexpected_input("XmlExprOp"),
        }
    }
}

impl XmlOptionType {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::XmlOptionType,
    ) -> Result<Self, AnalysisError> {
        match value {
            syn::XmlOptionType::XmloptionDocument => Ok(Self::Document),
            syn::XmlOptionType::XmloptionContent => Ok(Self::Content),
            _ => AnalysisError::raise_unexpected_input("XmlOptionType"),
        }
    }
}

impl MergeMatchKind {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::MergeMatchKind,
    ) -> Result<Self, AnalysisError> {
        match value {
            syn::MergeMatchKind::MergeWhenMatched => Ok(Self::Matched),
            syn::MergeMatchKind::MergeWhenNotMatchedBySource => Ok(Self::NotMatchedBySource),
            syn::MergeMatchKind::MergeWhenNotMatchedByTarget => Ok(Self::NotMatchedByTarget),
            _ => AnalysisError::raise_unexpected_input("MergeMatchKind"),
        }
    }
}

impl CmdType {
    pub(in crate::sem::unresolved) fn postgres(value: syn::CmdType) -> Result<Self, AnalysisError> {
        match value {
            syn::CmdType::CmdUnknown => Ok(Self::Unknown),
            syn::CmdType::CmdSelect => Ok(Self::Select),
            syn::CmdType::CmdUpdate => Ok(Self::Update),
            syn::CmdType::CmdInsert => Ok(Self::Insert),
            syn::CmdType::CmdDelete => Ok(Self::Delete),
            syn::CmdType::CmdMerge => Ok(Self::Merge),
            syn::CmdType::CmdUtility => Ok(Self::Utility),
            syn::CmdType::CmdNothing => Ok(Self::Nothing),
            _ => AnalysisError::raise_unexpected_input("CmdType"),
        }
    }
}

impl OverridingKind {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::OverridingKind,
    ) -> Result<Self, AnalysisError> {
        match value {
            syn::OverridingKind::OverridingNotSet => Ok(Self::NotSet),
            syn::OverridingKind::OverridingUserValue => Ok(Self::UserValue),
            syn::OverridingKind::OverridingSystemValue => Ok(Self::SystemValue),
            _ => AnalysisError::raise_unexpected_input("OverridingKind"),
        }
    }
}

impl JsonFormat {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonFormat,
        _tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            format_type: JsonFormatType::postgres(required(
                value.get_format_type().as_inner(),
                "JsonFormatType",
            )?)?,
            encoding: JsonEncoding::postgres(required(
                value.get_encoding().as_inner(),
                "JsonEncoding",
            )?)?,
            location: location(value.get_location()),
        })
    }
}

impl JsonOutput {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonOutput,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            type_name: value
                .get_type_name()
                .as_inner()
                .map(|value| TypeName::postgres(&value, tokens))
                .transpose()?,
            returning: value
                .get_returning()
                .as_inner()
                .map(|value| JsonReturning::postgres(&value, tokens))
                .transpose()?,
        })
    }
}

impl JsonReturning {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonReturning,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            format: value
                .get_format()
                .as_inner()
                .map(|value| JsonFormat::postgres(&value, tokens))
                .transpose()?,
        })
    }
}

impl JsonValue {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonValueExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            raw_expr: Expr::postgres(&value.get_raw_expr(), tokens)?,
            format: value
                .get_format()
                .as_inner()
                .map(|value| JsonFormat::postgres(&value, tokens))
                .transpose()?,
        })
    }

    fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut values = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            values.push(Self::postgres(
                &required(node.as_json_value_expr().as_inner(), "JsonValueExpr")?,
                tokens,
            )?);
        }
        Ok(values)
    }
}

impl JsonArgument {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonArgument,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            val: JsonValue::postgres(&required(value.get_val().as_inner(), "JsonValue")?, tokens)?,
            name: value.get_name(),
        })
    }

    fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut values = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            values.push(Self::postgres(
                &required(node.as_json_argument().as_inner(), "JsonArgument")?,
                tokens,
            )?);
        }
        Ok(values)
    }
}

impl JsonBehavior {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonBehavior,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            btype: JsonBehaviorType::postgres(required(
                value.get_btype().as_inner(),
                "JsonBehaviorType",
            )?)?,
            expr: Expr::postgres_optional(value.get_expr(), tokens)?,
            location: location(value.get_location()),
        })
    }
}

impl JsonPath {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonTablePathSpec,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            string: Expr::postgres(&value.get_string(), tokens)?,
            name: value.get_name(),
            name_location: location(value.get_name_location()),
            location: location(value.get_location()),
        })
    }
}

impl JsonKeyValue {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonKeyValue,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            key: Expr::postgres(&value.get_key(), tokens)?,
            value: JsonValue::postgres(
                &required(value.get_value().as_inner(), "JsonValue")?,
                tokens,
            )?,
        })
    }

    fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut values = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            values.push(Self::postgres(
                &required(node.as_json_key_value().as_inner(), "JsonKeyValue")?,
                tokens,
            )?);
        }
        Ok(values)
    }
}

impl JsonFunction {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonFuncExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            op: JsonExprOp::postgres(required(value.get_op().as_inner(), "JsonExprOp")?)?,
            column_name: value.get_column_name(),
            context_item: JsonValue::postgres(
                &required(value.get_context_item().as_inner(), "JsonValue")?,
                tokens,
            )?,
            pathspec: Expr::postgres(&value.get_pathspec(), tokens)?,
            passing: JsonArgument::postgres_list(value.get_passing(), tokens)?,
            output: value
                .get_output()
                .as_inner()
                .map(|value| JsonOutput::postgres(&value, tokens))
                .transpose()?,
            on_empty: value
                .get_on_empty()
                .as_inner()
                .map(|value| JsonBehavior::postgres(&value, tokens))
                .transpose()?,
            on_error: value
                .get_on_error()
                .as_inner()
                .map(|value| JsonBehavior::postgres(&value, tokens))
                .transpose()?,
            wrapper: JsonWrapper::postgres(required(
                value.get_wrapper().as_inner(),
                "JsonWrapper",
            )?)?,
            quotes: JsonQuotes::postgres(required(value.get_quotes().as_inner(), "JsonQuotes")?)?,
            location: location(value.get_location()),
        })
    }
}

impl JsonPredicate {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonIsPredicate,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            expr: Expr::postgres(&value.get_expr(), tokens)?,
            format: value
                .get_format()
                .as_inner()
                .map(|value| JsonFormat::postgres(&value, tokens))
                .transpose()?,
            item_type: JsonValueType::postgres(required(
                value.get_item_type().as_inner(),
                "JsonValueType",
            )?)?,
            unique_keys: value.get_unique_keys(),
            location: location(value.get_location()),
        })
    }
}

impl JsonParse {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonParseExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            expr: JsonValue::postgres(
                &required(value.get_expr().as_inner(), "JsonValue")?,
                tokens,
            )?,
            output: value
                .get_output()
                .as_inner()
                .map(|value| JsonOutput::postgres(&value, tokens))
                .transpose()?,
            unique_keys: value.get_unique_keys(),
            location: location(value.get_location()),
        })
    }
}

impl JsonScalar {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonScalarExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            expr: Expr::postgres(&value.get_expr(), tokens)?,
            output: value
                .get_output()
                .as_inner()
                .map(|value| JsonOutput::postgres(&value, tokens))
                .transpose()?,
            location: location(value.get_location()),
        })
    }
}

impl JsonSerialize {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonSerializeExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            expr: JsonValue::postgres(
                &required(value.get_expr().as_inner(), "JsonValue")?,
                tokens,
            )?,
            output: value
                .get_output()
                .as_inner()
                .map(|value| JsonOutput::postgres(&value, tokens))
                .transpose()?,
            location: location(value.get_location()),
        })
    }
}

impl JsonObject {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonObjectConstructor,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            exprs: JsonKeyValue::postgres_list(value.get_exprs(), tokens)?,
            output: value
                .get_output()
                .as_inner()
                .map(|value| JsonOutput::postgres(&value, tokens))
                .transpose()?,
            absent_on_null: value.get_absent_on_null(),
            unique: value.get_unique(),
            location: location(value.get_location()),
        })
    }
}

impl JsonArray {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonArrayConstructor,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            exprs: JsonValue::postgres_list(value.get_exprs(), tokens)?,
            output: value
                .get_output()
                .as_inner()
                .map(|value| JsonOutput::postgres(&value, tokens))
                .transpose()?,
            absent_on_null: value.get_absent_on_null(),
            location: location(value.get_location()),
        })
    }
}

impl JsonArrayQuery {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonArrayQueryConstructor,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            query: Query::postgres(
                &required(value.get_query().as_select_stmt().as_inner(), "query")?,
                tokens,
            )?,
            output: value
                .get_output()
                .as_inner()
                .map(|value| JsonOutput::postgres(&value, tokens))
                .transpose()?,
            format: value
                .get_format()
                .as_inner()
                .map(|value| JsonFormat::postgres(&value, tokens))
                .transpose()?,
            absent_on_null: value.get_absent_on_null(),
            location: location(value.get_location()),
        })
    }
}

impl JsonAggregate {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonAggConstructor,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            output: value
                .get_output()
                .as_inner()
                .map(|value| JsonOutput::postgres(&value, tokens))
                .transpose()?,
            agg_filter: Expr::postgres_optional(value.get_agg_filter(), tokens)?,
            agg_order: Order::postgres_list(value.get_agg_order(), tokens)?,
            over: value
                .get_over()
                .as_inner()
                .map(|value| Window::postgres(&value, tokens))
                .transpose()?,
            location: location(value.get_location()),
        })
    }
}

impl JsonObjectAggregate {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonObjectAgg,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            constructor: JsonAggregate::postgres(
                &required(value.get_constructor().as_inner(), "JsonAggregate")?,
                tokens,
            )?,
            arg: JsonKeyValue::postgres(
                &required(value.get_arg().as_inner(), "JsonKeyValue")?,
                tokens,
            )?,
            absent_on_null: value.get_absent_on_null(),
            unique: value.get_unique(),
        })
    }
}

impl JsonArrayAggregate {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonArrayAgg,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            constructor: JsonAggregate::postgres(
                &required(value.get_constructor().as_inner(), "JsonAggregate")?,
                tokens,
            )?,
            arg: JsonValue::postgres(&required(value.get_arg().as_inner(), "JsonValue")?, tokens)?,
            absent_on_null: value.get_absent_on_null(),
        })
    }
}

impl JsonTable {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonTable,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            context_item: JsonValue::postgres(
                &required(value.get_context_item().as_inner(), "JsonValue")?,
                tokens,
            )?,
            pathspec: JsonPath::postgres(
                &required(value.get_pathspec().as_inner(), "JsonPath")?,
                tokens,
            )?,
            passing: JsonArgument::postgres_list(value.get_passing(), tokens)?,
            columns: JsonColumn::postgres_list(value.get_columns(), tokens)?,
            on_error: value
                .get_on_error()
                .as_inner()
                .map(|value| JsonBehavior::postgres(&value, tokens))
                .transpose()?,
            alias: Alias::postgres(value.get_alias())?,
            lateral: value.get_lateral(),
            location: location(value.get_location()),
        })
    }
}

impl JsonColumn {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::JsonTableColumn,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            coltype: JsonTableColumnType::postgres(required(
                value.get_coltype().as_inner(),
                "JsonTableColumnType",
            )?)?,
            name: value.get_name(),
            type_name: value
                .get_type_name()
                .as_inner()
                .map(|value| TypeName::postgres(&value, tokens))
                .transpose()?,
            pathspec: value
                .get_pathspec()
                .as_inner()
                .map(|value| JsonPath::postgres(&value, tokens))
                .transpose()?,
            format: value
                .get_format()
                .as_inner()
                .map(|value| JsonFormat::postgres(&value, tokens))
                .transpose()?,
            wrapper: JsonWrapper::postgres(required(
                value.get_wrapper().as_inner(),
                "JsonWrapper",
            )?)?,
            quotes: JsonQuotes::postgres(required(value.get_quotes().as_inner(), "JsonQuotes")?)?,
            columns: Self::postgres_list(value.get_columns(), tokens)?,
            on_empty: value
                .get_on_empty()
                .as_inner()
                .map(|value| JsonBehavior::postgres(&value, tokens))
                .transpose()?,
            on_error: value
                .get_on_error()
                .as_inner()
                .map(|value| JsonBehavior::postgres(&value, tokens))
                .transpose()?,
            location: location(value.get_location()),
        })
    }

    fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut values = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            values.push(Self::postgres(
                &required(node.as_json_table_column().as_inner(), "JsonTableColumn")?,
                tokens,
            )?);
        }
        Ok(values)
    }
}

impl Xml {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::XmlExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            op: XmlExprOp::postgres(required(value.get_op().as_inner(), "XmlExprOp")?)?,
            name: value.get_name(),
            named_args: Target::postgres_list(value.get_named_args(), tokens)?,
            arg_names: Name::postgres(value.get_arg_names())?,
            args: Expr::postgres_list(value.get_args(), tokens)?,
            xmloption: XmlOptionType::postgres(required(
                value.get_xmloption().as_inner(),
                "XmlOptionType",
            )?)?,
            indent: value.get_indent(),
            location: location(value.get_location()),
        })
    }
}

impl XmlSerialize {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::XmlSerialize,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            xmloption: XmlOptionType::postgres(required(
                value.get_xmloption().as_inner(),
                "XmlOptionType",
            )?)?,
            expr: Expr::postgres(&value.get_expr(), tokens)?,
            type_name: TypeName::postgres(
                &required(value.get_type_name().as_inner(), "TypeName")?,
                tokens,
            )?,
            indent: value.get_indent(),
            location: location(value.get_location()),
        })
    }
}

impl XmlTable {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::RangeTableFunc,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            lateral: value.get_lateral(),
            docexpr: Expr::postgres_optional(value.get_docexpr(), tokens)?,
            rowexpr: Expr::postgres(&value.get_rowexpr(), tokens)?,
            namespaces: Target::postgres_list(value.get_namespaces(), tokens)?,
            columns: XmlColumn::postgres_list(value.get_columns(), tokens)?,
            alias: Alias::postgres(value.get_alias())?,
            location: location(value.get_location()),
        })
    }
}

impl XmlColumn {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::RangeTableFuncCol,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            colname: value.get_colname(),
            type_name: value
                .get_type_name()
                .as_inner()
                .map(|value| TypeName::postgres(&value, tokens))
                .transpose()?,
            for_ordinality: value.get_for_ordinality(),
            is_not_null: value.get_is_not_null(),
            colexpr: Expr::postgres_optional(value.get_colexpr(), tokens)?,
            coldefexpr: Expr::postgres_optional(value.get_coldefexpr(), tokens)?,
            location: location(value.get_location()),
        })
    }

    fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut values = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            values.push(Self::postgres(
                &required(
                    node.as_range_table_func_col().as_inner(),
                    "RangeTableFuncCol",
                )?,
                tokens,
            )?);
        }
        Ok(values)
    }
}

impl MultiAssignment {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::MultiAssignRef,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            source: Expr::postgres(&value.get_source(), tokens)?,
            colno: value.get_colno(),
            ncolumns: value.get_ncolumns(),
        })
    }
}

impl Merge {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::MergeStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            relation: Relation::postgres(&required(value.get_relation().as_inner(), "relation")?)?,
            source_relation: FromItem::postgres(&value.get_source_relation(), tokens)?,
            join_condition: Expr::postgres(&value.get_join_condition(), tokens)?,
            merge_when_clauses: MergeWhen::postgres_list(value.get_merge_when_clauses(), tokens)?,
            returning_list: Target::postgres_list(value.get_returning_list(), tokens)?,
            with_clause: With::postgres(value.get_with_clause(), tokens)?,
        })
    }
}

impl MergeWhen {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::MergeWhenClause,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            match_kind: MergeMatchKind::postgres(required(
                value.get_match_kind().as_inner(),
                "MergeMatchKind",
            )?)?,
            command_type: CmdType::postgres(required(
                value.get_command_type().as_inner(),
                "CmdType",
            )?)?,
            overriding: OverridingKind::postgres(required(
                value.get_override().as_inner(),
                "OverridingKind",
            )?)?,
            condition: Expr::postgres_optional(value.get_condition(), tokens)?,
            target_list: Assignment::postgres_list(value.get_target_list(), tokens)?,
            values: Expr::postgres_list(value.get_values(), tokens)?,
        })
    }

    fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut values = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            values.push(Self::postgres(
                &required(node.as_merge_when_clause().as_inner(), "MergeWhenClause")?,
                tokens,
            )?);
        }
        Ok(values)
    }
}

fn location(offset: i32) -> Option<StringSpan> {
    usize::try_from(offset)
        .ok()
        .map(|offset| StringSpan::new(offset, 0))
}

impl DocumentExpr {
    pub(in crate::sem::unresolved) fn postgres(
        node: &syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Option<Self>, AnalysisError> {
        if let Some(value) = node.as_json_func_expr().as_inner() {
            return Ok(Some(Self::JsonFunction(Box::new(JsonFunction::postgres(
                &value, tokens,
            )?))));
        }
        if let Some(value) = node.as_json_is_predicate().as_inner() {
            return Ok(Some(Self::JsonPredicate(Box::new(
                JsonPredicate::postgres(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_json_parse_expr().as_inner() {
            return Ok(Some(Self::JsonParse(Box::new(JsonParse::postgres(
                &value, tokens,
            )?))));
        }
        if let Some(value) = node.as_json_scalar_expr().as_inner() {
            return Ok(Some(Self::JsonScalar(Box::new(JsonScalar::postgres(
                &value, tokens,
            )?))));
        }
        if let Some(value) = node.as_json_serialize_expr().as_inner() {
            return Ok(Some(Self::JsonSerialize(Box::new(
                JsonSerialize::postgres(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_json_object_constructor().as_inner() {
            return Ok(Some(Self::JsonObject(Box::new(JsonObject::postgres(
                &value, tokens,
            )?))));
        }
        if let Some(value) = node.as_json_array_constructor().as_inner() {
            return Ok(Some(Self::JsonArray(Box::new(JsonArray::postgres(
                &value, tokens,
            )?))));
        }
        if let Some(value) = node.as_json_array_query_constructor().as_inner() {
            return Ok(Some(Self::JsonArrayQuery(Box::new(
                JsonArrayQuery::postgres(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_json_object_agg().as_inner() {
            return Ok(Some(Self::JsonObjectAggregate(Box::new(
                JsonObjectAggregate::postgres(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_json_array_agg().as_inner() {
            return Ok(Some(Self::JsonArrayAggregate(Box::new(
                JsonArrayAggregate::postgres(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_xml_expr().as_inner() {
            return Ok(Some(Self::Xml(Box::new(Xml::postgres(&value, tokens)?))));
        }
        if let Some(value) = node.as_xml_serialize().as_inner() {
            return Ok(Some(Self::XmlSerialize(Box::new(XmlSerialize::postgres(
                &value, tokens,
            )?))));
        }
        if let Some(value) = node.as_multi_assign_ref().as_inner() {
            return Ok(Some(Self::MultiAssignment(Box::new(
                MultiAssignment::postgres(&value, tokens)?,
            ))));
        }
        Ok(None)
    }
}
