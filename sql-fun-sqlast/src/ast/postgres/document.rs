use crate::ast::document::DocumentExpr;
use crate::ast::postgres::required;
use crate::ast::{
    Alias, Assignment, CmdType, Expr, FromItem, JsonAggregate, JsonArgument, JsonArray,
    JsonArrayAggregate, JsonArrayQuery, JsonBehavior, JsonBehaviorType, JsonColumn, JsonEncoding,
    JsonExprOp, JsonFormat, JsonFormatType, JsonFunction, JsonKeyValue, JsonObject,
    JsonObjectAggregate, JsonOutput, JsonParse, JsonPath, JsonPredicate, JsonQuotes, JsonReturning,
    JsonScalar, JsonSerialize, JsonTable, JsonTableColumnType, JsonValue, JsonValueType,
    JsonWrapper, Merge, MergeMatchKind, MergeWhen, MultiAssignment, Name, Order, OverridingKind,
    Query, Relation, Target, TryCollectFromWithTokens, TryFromWithTokens, TryFromWithTokensOpt,
    TypeName, Window, With, Xml, XmlColumn, XmlExprOp, XmlOptionType, XmlSerialize, XmlTable,
};
use crate::syn::{ListOpt, Opt};
use crate::{StringSpan, sem::AnalysisError, syn};

use ::sql_fun_core::IVec;

impl TryFrom<syn::JsonExprOp> for JsonExprOp {
    type Error = AnalysisError;

    fn try_from(value: syn::JsonExprOp) -> Result<Self, Self::Error> {
        match value {
            syn::JsonExprOp::JsonExistsOp => Ok(Self::Exists),
            syn::JsonExprOp::JsonQueryOp => Ok(Self::Query),
            syn::JsonExprOp::JsonValueOp => Ok(Self::Value),
            syn::JsonExprOp::JsonTableOp => Ok(Self::Table),
            _ => AnalysisError::raise_unexpected_input("JsonExprOp"),
        }
    }
}

impl TryFrom<syn::JsonFormatType> for JsonFormatType {
    type Error = AnalysisError;

    fn try_from(value: syn::JsonFormatType) -> Result<Self, Self::Error> {
        match value {
            syn::JsonFormatType::JsFormatDefault => Ok(Self::Default),
            syn::JsonFormatType::JsFormatJson => Ok(Self::Json),
            syn::JsonFormatType::JsFormatJsonb => Ok(Self::Jsonb),
            _ => AnalysisError::raise_unexpected_input("JsonFormatType"),
        }
    }
}

impl TryFrom<syn::JsonEncoding> for JsonEncoding {
    type Error = AnalysisError;

    fn try_from(value: syn::JsonEncoding) -> Result<Self, Self::Error> {
        match value {
            syn::JsonEncoding::JsEncDefault => Ok(Self::Default),
            syn::JsonEncoding::JsEncUtf8 => Ok(Self::Utf8),
            syn::JsonEncoding::JsEncUtf16 => Ok(Self::Utf16),
            syn::JsonEncoding::JsEncUtf32 => Ok(Self::Utf32),
            _ => AnalysisError::raise_unexpected_input("JsonEncoding"),
        }
    }
}

impl TryFrom<syn::JsonValueType> for JsonValueType {
    type Error = AnalysisError;

    fn try_from(value: syn::JsonValueType) -> Result<Self, Self::Error> {
        match value {
            syn::JsonValueType::JsTypeAny => Ok(Self::Any),
            syn::JsonValueType::JsTypeObject => Ok(Self::Object),
            syn::JsonValueType::JsTypeArray => Ok(Self::Array),
            syn::JsonValueType::JsTypeScalar => Ok(Self::Scalar),
            _ => AnalysisError::raise_unexpected_input("JsonValueType"),
        }
    }
}

impl TryFrom<syn::JsonBehaviorType> for JsonBehaviorType {
    type Error = AnalysisError;

    fn try_from(value: syn::JsonBehaviorType) -> Result<Self, AnalysisError> {
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

impl TryFrom<syn::JsonWrapper> for JsonWrapper {
    type Error = AnalysisError;

    fn try_from(value: syn::JsonWrapper) -> Result<Self, Self::Error> {
        match value {
            syn::JsonWrapper::JswUnspec => Ok(Self::Unspec),
            syn::JsonWrapper::JswNone => Ok(Self::None),
            syn::JsonWrapper::JswConditional => Ok(Self::Conditional),
            syn::JsonWrapper::JswUnconditional => Ok(Self::Unconditional),
            _ => AnalysisError::raise_unexpected_input("JsonWrapper"),
        }
    }
}

impl TryFrom<syn::JsonQuotes> for JsonQuotes {
    type Error = AnalysisError;

    fn try_from(value: syn::JsonQuotes) -> Result<Self, Self::Error> {
        match value {
            syn::JsonQuotes::JsQuotesUnspec => Ok(Self::Unspec),
            syn::JsonQuotes::JsQuotesKeep => Ok(Self::Keep),
            syn::JsonQuotes::JsQuotesOmit => Ok(Self::Omit),
            _ => AnalysisError::raise_unexpected_input("JsonQuotes"),
        }
    }
}

impl TryFrom<syn::JsonTableColumnType> for JsonTableColumnType {
    type Error = AnalysisError;

    fn try_from(value: syn::JsonTableColumnType) -> Result<Self, Self::Error> {
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

impl TryFrom<syn::XmlExprOp> for XmlExprOp {
    type Error = AnalysisError;

    fn try_from(value: syn::XmlExprOp) -> Result<Self, Self::Error> {
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

impl TryFrom<syn::XmlOptionType> for XmlOptionType {
    type Error = AnalysisError;

    fn try_from(value: syn::XmlOptionType) -> Result<Self, Self::Error> {
        match value {
            syn::XmlOptionType::XmloptionDocument => Ok(Self::Document),
            syn::XmlOptionType::XmloptionContent => Ok(Self::Content),
            _ => AnalysisError::raise_unexpected_input("XmlOptionType"),
        }
    }
}

impl TryFrom<syn::MergeMatchKind> for MergeMatchKind {
    type Error = AnalysisError;

    fn try_from(value: syn::MergeMatchKind) -> Result<Self, Self::Error> {
        match value {
            syn::MergeMatchKind::MergeWhenMatched => Ok(Self::Matched),
            syn::MergeMatchKind::MergeWhenNotMatchedBySource => Ok(Self::NotMatchedBySource),
            syn::MergeMatchKind::MergeWhenNotMatchedByTarget => Ok(Self::NotMatchedByTarget),
            _ => AnalysisError::raise_unexpected_input("MergeMatchKind"),
        }
    }
}

impl TryFrom<syn::CmdType> for CmdType {
    type Error = AnalysisError;

    fn try_from(value: syn::CmdType) -> Result<Self, Self::Error> {
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

impl TryFrom<syn::OverridingKind> for OverridingKind {
    type Error = AnalysisError;
    fn try_from(value: syn::OverridingKind) -> Result<Self, Self::Error> {
        match value {
            syn::OverridingKind::OverridingNotSet => Ok(Self::NotSet),
            syn::OverridingKind::OverridingUserValue => Ok(Self::UserValue),
            syn::OverridingKind::OverridingSystemValue => Ok(Self::SystemValue),
            _ => AnalysisError::raise_unexpected_input("OverridingKind"),
        }
    }
}

impl TryFromWithTokensOpt<syn::JsonFormatOpt> for JsonFormat {
    type Error = AnalysisError;

    fn try_from_with_tokens_opt(
        value: syn::JsonFormatOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Option<Self>, Self::Error> {
        value
            .as_inner()
            .map(|v| Self::try_from_with_tokens(&v, tokens))
            .transpose()
    }
}

impl TryFromWithTokens<&syn::JsonFormat> for JsonFormat {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonFormat,
        _tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            format_type: JsonFormatType::try_from(required(
                value.get_format_type().as_inner(),
                "JsonFormatType",
            )?)?,
            encoding: JsonEncoding::try_from(required(
                value.get_encoding().as_inner(),
                "JsonEncoding",
            )?)?,
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokensOpt<syn::JsonOutputOpt> for JsonOutput {
    type Error = AnalysisError;

    fn try_from_with_tokens_opt(
        value: syn::JsonOutputOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Option<Self>, Self::Error> {
        value
            .as_inner()
            .map(|v| Self::try_from_with_tokens(&v, tokens))
            .transpose()
    }
}

impl TryFromWithTokens<&syn::JsonOutput> for JsonOutput {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonOutput,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            type_name: value
                .get_type_name()
                .as_inner()
                .map(|value| TypeName::try_from_with_tokens(&value, tokens))
                .transpose()?,
            returning: JsonReturning::try_from_with_tokens_opt(value.get_returning(), tokens)?,
        })
    }
}

impl TryFromWithTokensOpt<syn::JsonReturningOpt> for JsonReturning {
    type Error = AnalysisError;

    fn try_from_with_tokens_opt(
        value: syn::JsonReturningOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Option<Self>, Self::Error> {
        value
            .as_inner()
            .map(|v| Self::try_from_with_tokens(&v, tokens))
            .transpose()
    }
}

impl TryFromWithTokens<&syn::JsonReturning> for JsonReturning {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonReturning,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            format: JsonFormat::try_from_with_tokens_opt(value.get_format(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::JsonValueExpr> for JsonValue {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonValueExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            raw_expr: Expr::try_from_with_tokens(&value.get_raw_expr(), tokens)?,
            format: JsonFormat::try_from_with_tokens_opt(value.get_format(), tokens)?,
        })
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for JsonValue {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut values = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            values.push(Self::try_from_with_tokens(
                &required(node.as_json_value_expr().as_inner(), "JsonValueExpr")?,
                tokens,
            )?);
        }
        Ok(values)
    }
}

impl TryFromWithTokens<&syn::JsonArgument> for JsonArgument {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonArgument,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            val: JsonValue::try_from_with_tokens(
                &required(value.get_val().as_inner(), "JsonValue")?,
                tokens,
            )?,
            name: value.get_name(),
        })
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for JsonArgument {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut values = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            values.push(Self::try_from_with_tokens(
                &required(node.as_json_argument().as_inner(), "JsonArgument")?,
                tokens,
            )?);
        }
        Ok(values)
    }
}

impl TryFromWithTokensOpt<syn::JsonBehaviorOpt> for JsonBehavior {
    type Error = AnalysisError;

    fn try_from_with_tokens_opt(
        value: syn::JsonBehaviorOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Option<Self>, Self::Error> {
        value
            .as_inner()
            .map(|v| Self::try_from_with_tokens(&v, tokens))
            .transpose()
    }
}

impl TryFromWithTokens<&syn::JsonBehavior> for JsonBehavior {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonBehavior,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            btype: JsonBehaviorType::try_from(required(
                value.get_btype().as_inner(),
                "JsonBehaviorType",
            )?)?,
            expr: Expr::try_from_with_tokens_opt(value.get_expr(), tokens)?,
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::JsonTablePathSpec> for JsonPath {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonTablePathSpec,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            string: Expr::try_from_with_tokens(&value.get_string(), tokens)?,
            name: value.get_name(),
            name_location: location(value.get_name_location()),
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::JsonKeyValue> for JsonKeyValue {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonKeyValue,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            key: Expr::try_from_with_tokens(&value.get_key(), tokens)?,
            value: JsonValue::try_from_with_tokens(
                &required(value.get_value().as_inner(), "JsonValue")?,
                tokens,
            )?,
        })
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for JsonKeyValue {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut values = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            values.push(Self::try_from_with_tokens(
                &required(node.as_json_key_value().as_inner(), "JsonKeyValue")?,
                tokens,
            )?);
        }
        Ok(values)
    }
}

impl TryFromWithTokens<&syn::JsonFuncExpr> for JsonFunction {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::JsonFuncExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            op: JsonExprOp::try_from(required(value.get_op().as_inner(), "JsonExprOp")?)?,
            column_name: value.get_column_name(),
            context_item: JsonValue::try_from_with_tokens(
                &required(value.get_context_item().as_inner(), "JsonValue")?,
                tokens,
            )?,
            pathspec: Expr::try_from_with_tokens(&value.get_pathspec(), tokens)?,
            passing: JsonArgument::try_collect_from_with_tokens(value.get_passing(), tokens)?,
            output: JsonOutput::try_from_with_tokens_opt(value.get_output(), tokens)?,
            on_empty: JsonBehavior::try_from_with_tokens_opt(value.get_on_empty(), tokens)?,
            on_error: JsonBehavior::try_from_with_tokens_opt(value.get_on_error(), tokens)?,
            wrapper: JsonWrapper::try_from(required(
                value.get_wrapper().as_inner(),
                "JsonWrapper",
            )?)?,
            quotes: JsonQuotes::try_from(required(value.get_quotes().as_inner(), "JsonQuotes")?)?,
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::JsonIsPredicate> for JsonPredicate {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonIsPredicate,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            expr: Expr::try_from_with_tokens(&value.get_expr(), tokens)?,
            format: JsonFormat::try_from_with_tokens_opt(value.get_format(), tokens)?,
            item_type: JsonValueType::try_from(required(
                value.get_item_type().as_inner(),
                "JsonValueType",
            )?)?,
            unique_keys: value.get_unique_keys(),
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::JsonParseExpr> for JsonParse {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonParseExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            expr: JsonValue::try_from_with_tokens(
                &required(value.get_expr().as_inner(), "JsonValue")?,
                tokens,
            )?,
            output: JsonOutput::try_from_with_tokens_opt(value.get_output(), tokens)?,
            unique_keys: value.get_unique_keys(),
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::JsonScalarExpr> for JsonScalar {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonScalarExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            expr: Expr::try_from_with_tokens(&value.get_expr(), tokens)?,
            output: JsonOutput::try_from_with_tokens_opt(value.get_output(), tokens)?,
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::JsonSerializeExpr> for JsonSerialize {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonSerializeExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            expr: JsonValue::try_from_with_tokens(
                &required(value.get_expr().as_inner(), "JsonValue")?,
                tokens,
            )?,
            output: JsonOutput::try_from_with_tokens_opt(value.get_output(), tokens)?,
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::JsonObjectConstructor> for JsonObject {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonObjectConstructor,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            exprs: JsonKeyValue::try_collect_from_with_tokens(value.get_exprs(), tokens)?,
            output: JsonOutput::try_from_with_tokens_opt(value.get_output(), tokens)?,
            absent_on_null: value.get_absent_on_null(),
            unique: value.get_unique(),
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::JsonArrayConstructor> for JsonArray {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonArrayConstructor,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            exprs: JsonValue::try_collect_from_with_tokens(value.get_exprs(), tokens)?,
            output: JsonOutput::try_from_with_tokens_opt(value.get_output(), tokens)?,
            absent_on_null: value.get_absent_on_null(),
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::JsonArrayQueryConstructor> for JsonArrayQuery {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonArrayQueryConstructor,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            query: Query::try_from_with_tokens(
                &required(value.get_query().as_select_stmt().as_inner(), "query")?,
                tokens,
            )?,
            output: JsonOutput::try_from_with_tokens_opt(value.get_output(), tokens)?,
            format: JsonFormat::try_from_with_tokens_opt(value.get_format(), tokens)?,
            absent_on_null: value.get_absent_on_null(),
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::JsonAggConstructor> for JsonAggregate {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonAggConstructor,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            output: JsonOutput::try_from_with_tokens_opt(value.get_output(), tokens)?,
            agg_filter: Expr::try_from_with_tokens_opt(value.get_agg_filter(), tokens)?,
            agg_order: Order::try_collect_from_with_tokens(value.get_agg_order(), tokens)?,
            over: value
                .get_over()
                .as_inner()
                .map(|value| Window::try_from_with_tokens(&value, tokens))
                .transpose()?,
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::JsonObjectAgg> for JsonObjectAggregate {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonObjectAgg,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            constructor: JsonAggregate::try_from_with_tokens(
                &required(value.get_constructor().as_inner(), "JsonAggregate")?,
                tokens,
            )?,
            arg: JsonKeyValue::try_from_with_tokens(
                &required(value.get_arg().as_inner(), "JsonKeyValue")?,
                tokens,
            )?,
            absent_on_null: value.get_absent_on_null(),
            unique: value.get_unique(),
        })
    }
}

impl TryFromWithTokens<&syn::JsonArrayAgg> for JsonArrayAggregate {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonArrayAgg,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            constructor: JsonAggregate::try_from_with_tokens(
                &required(value.get_constructor().as_inner(), "JsonAggregate")?,
                tokens,
            )?,
            arg: JsonValue::try_from_with_tokens(
                &required(value.get_arg().as_inner(), "JsonValue")?,
                tokens,
            )?,
            absent_on_null: value.get_absent_on_null(),
        })
    }
}

impl TryFromWithTokens<&syn::JsonTable> for JsonTable {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonTable,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            context_item: JsonValue::try_from_with_tokens(
                &required(value.get_context_item().as_inner(), "JsonValue")?,
                tokens,
            )?,
            pathspec: JsonPath::try_from_with_tokens(
                &required(value.get_pathspec().as_inner(), "JsonPath")?,
                tokens,
            )?,
            passing: JsonArgument::try_collect_from_with_tokens(value.get_passing(), tokens)?,
            columns: JsonColumn::try_collect_from_with_tokens(value.get_columns(), tokens)?,
            on_error: JsonBehavior::try_from_with_tokens_opt(value.get_on_error(), tokens)?,
            alias: Alias::postgres(value.get_alias())?,
            lateral: value.get_lateral(),
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::JsonTableColumn> for JsonColumn {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::JsonTableColumn,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            coltype: JsonTableColumnType::try_from(required(
                value.get_coltype().as_inner(),
                "JsonTableColumnType",
            )?)?,
            name: value.get_name(),
            type_name: value
                .get_type_name()
                .as_inner()
                .map(|value| TypeName::try_from_with_tokens(&value, tokens))
                .transpose()?,
            pathspec: value
                .get_pathspec()
                .as_inner()
                .map(|value| JsonPath::try_from_with_tokens(&value, tokens))
                .transpose()?,
            format: JsonFormat::try_from_with_tokens_opt(value.get_format(), tokens)?,
            wrapper: JsonWrapper::try_from(required(
                value.get_wrapper().as_inner(),
                "JsonWrapper",
            )?)?,
            quotes: JsonQuotes::try_from(required(value.get_quotes().as_inner(), "JsonQuotes")?)?,
            columns: Self::try_collect_from_with_tokens(value.get_columns(), tokens)?,
            on_empty: JsonBehavior::try_from_with_tokens_opt(value.get_on_empty(), tokens)?,
            on_error: JsonBehavior::try_from_with_tokens_opt(value.get_on_error(), tokens)?,
            location: location(value.get_location()),
        })
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for JsonColumn {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut values = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            values.push(Self::try_from_with_tokens(
                &required(node.as_json_table_column().as_inner(), "JsonTableColumn")?,
                tokens,
            )?);
        }
        Ok(values)
    }
}

impl TryFromWithTokens<&syn::XmlExpr> for Xml {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::XmlExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            op: XmlExprOp::try_from(required(value.get_op().as_inner(), "XmlExprOp")?)?,
            name: value.get_name(),
            named_args: Target::try_collect_from_with_tokens(value.get_named_args(), tokens)?,
            arg_names: Name::try_from(value.get_arg_names())?,
            args: Expr::try_collect_from_with_tokens(value.get_args(), tokens)?,
            xmloption: XmlOptionType::try_from(required(
                value.get_xmloption().as_inner(),
                "XmlOptionType",
            )?)?,
            indent: value.get_indent(),
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::XmlSerialize> for XmlSerialize {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::XmlSerialize,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            xmloption: XmlOptionType::try_from(required(
                value.get_xmloption().as_inner(),
                "XmlOptionType",
            )?)?,
            expr: Expr::try_from_with_tokens(&value.get_expr(), tokens)?,
            type_name: TypeName::try_from_with_tokens(
                &required(value.get_type_name().as_inner(), "TypeName")?,
                tokens,
            )?,
            indent: value.get_indent(),
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::RangeTableFunc> for XmlTable {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::RangeTableFunc,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            lateral: value.get_lateral(),
            docexpr: Expr::try_from_with_tokens_opt(value.get_docexpr(), tokens)?,
            rowexpr: Expr::try_from_with_tokens(&value.get_rowexpr(), tokens)?,
            namespaces: Target::try_collect_from_with_tokens(value.get_namespaces(), tokens)?,
            columns: XmlColumn::try_collect_from_with_tokens(value.get_columns(), tokens)?,
            alias: Alias::postgres(value.get_alias())?,
            location: location(value.get_location()),
        })
    }
}

impl TryFromWithTokens<&syn::RangeTableFuncCol> for XmlColumn {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::RangeTableFuncCol,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            colname: value.get_colname(),
            type_name: value
                .get_type_name()
                .as_inner()
                .map(|value| TypeName::try_from_with_tokens(&value, tokens))
                .transpose()?,
            for_ordinality: value.get_for_ordinality(),
            is_not_null: value.get_is_not_null(),
            colexpr: Expr::try_from_with_tokens_opt(value.get_colexpr(), tokens)?,
            coldefexpr: Expr::try_from_with_tokens_opt(value.get_coldefexpr(), tokens)?,
            location: location(value.get_location()),
        })
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for XmlColumn {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut values = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            values.push(Self::try_from_with_tokens(
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

impl TryFromWithTokens<&syn::MultiAssignRef> for MultiAssignment {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::MultiAssignRef,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            source: Expr::try_from_with_tokens(&value.get_source(), tokens)?,
            colno: value.get_colno(),
            ncolumns: value.get_ncolumns(),
        })
    }
}

impl TryFromWithTokens<&syn::MergeStmt> for Merge {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::MergeStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            relation: Relation::try_from(&required(value.get_relation().as_inner(), "relation")?)?,
            source_relation: FromItem::try_from_with_tokens(&value.get_source_relation(), tokens)?,
            join_condition: Expr::try_from_with_tokens(&value.get_join_condition(), tokens)?,
            merge_when_clauses: MergeWhen::try_collect_from_with_tokens(
                value.get_merge_when_clauses(),
                tokens,
            )?,
            returning_options: crate::ast::ReturningOption::from_postgres(
                &value.get_returning_clause(),
            )?,
            returning_list: Target::try_collect_from_with_tokens(
                value.get_returning_clause().get_exprs(),
                tokens,
            )?,
            with_clause: With::try_from_with_tokens(value.get_with_clause(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::MergeWhenClause> for MergeWhen {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::MergeWhenClause,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            match_kind: MergeMatchKind::try_from(required(
                value.get_match_kind().as_inner(),
                "MergeMatchKind",
            )?)?,
            command_type: CmdType::try_from(required(
                value.get_command_type().as_inner(),
                "CmdType",
            )?)?,
            overriding: OverridingKind::try_from(required(
                value.get_override().as_inner(),
                "OverridingKind",
            )?)?,
            condition: Expr::try_from_with_tokens_opt(value.get_condition(), tokens)?,
            target_list: Assignment::try_collect_from_with_tokens(value.get_target_list(), tokens)?,
            values: Expr::try_collect_from_with_tokens(value.get_values(), tokens)?,
        })
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for MergeWhen {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut values = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            values.push(Self::try_from_with_tokens(
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

impl TryFromWithTokensOpt<&syn::Node> for DocumentExpr {
    type Error = AnalysisError;

    fn try_from_with_tokens_opt(
        node: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Option<Self>, Self::Error> {
        if let Some(value) = node.as_json_func_expr().as_inner() {
            return Ok(Some(Self::JsonFunction(Box::new(
                JsonFunction::try_from_with_tokens(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_json_is_predicate().as_inner() {
            return Ok(Some(Self::JsonPredicate(Box::new(
                JsonPredicate::try_from_with_tokens(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_json_parse_expr().as_inner() {
            return Ok(Some(Self::JsonParse(Box::new(
                JsonParse::try_from_with_tokens(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_json_scalar_expr().as_inner() {
            return Ok(Some(Self::JsonScalar(Box::new(
                JsonScalar::try_from_with_tokens(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_json_serialize_expr().as_inner() {
            return Ok(Some(Self::JsonSerialize(Box::new(
                JsonSerialize::try_from_with_tokens(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_json_object_constructor().as_inner() {
            return Ok(Some(Self::JsonObject(Box::new(
                JsonObject::try_from_with_tokens(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_json_array_constructor().as_inner() {
            return Ok(Some(Self::JsonArray(Box::new(
                JsonArray::try_from_with_tokens(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_json_array_query_constructor().as_inner() {
            return Ok(Some(Self::JsonArrayQuery(Box::new(
                JsonArrayQuery::try_from_with_tokens(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_json_object_agg().as_inner() {
            return Ok(Some(Self::JsonObjectAggregate(Box::new(
                JsonObjectAggregate::try_from_with_tokens(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_json_array_agg().as_inner() {
            return Ok(Some(Self::JsonArrayAggregate(Box::new(
                JsonArrayAggregate::try_from_with_tokens(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_xml_expr().as_inner() {
            return Ok(Some(Self::Xml(Box::new(Xml::try_from_with_tokens(
                &value, tokens,
            )?))));
        }
        if let Some(value) = node.as_xml_serialize().as_inner() {
            return Ok(Some(Self::XmlSerialize(Box::new(
                XmlSerialize::try_from_with_tokens(&value, tokens)?,
            ))));
        }
        if let Some(value) = node.as_multi_assign_ref().as_inner() {
            return Ok(Some(Self::MultiAssignment(Box::new(
                MultiAssignment::try_from_with_tokens(&value, tokens)?,
            ))));
        }
        Ok(None)
    }
}
