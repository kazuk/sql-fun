use super::{
    CmdType, DocumentExpr, JsonAggregate, JsonArgument, JsonArray, JsonArrayAggregate,
    JsonArrayQuery, JsonBehavior, JsonBehaviorType, JsonColumn, JsonEncoding, JsonExprOp,
    JsonFormat, JsonFormatType, JsonFunction, JsonKeyValue, JsonObject, JsonObjectAggregate,
    JsonOutput, JsonParse, JsonPath, JsonPredicate, JsonQuotes, JsonReturning, JsonScalar,
    JsonSerialize, JsonTable, JsonTableColumnType, JsonValue, JsonValueType, JsonWrapper, Merge,
    MergeMatchKind, MergeWhen, Xml, XmlColumn, XmlExprOp, XmlOptionType, XmlSerialize, XmlTable,
};
use crate::ast::Assignment;
use crate::ast::deparse::{DeparseError, SqlEmit, SqlWriter};

impl SqlEmit for DocumentExpr {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.postgres_only("document expression")?;
        match self {
            Self::JsonFunction(v) => v.emit(w),
            Self::JsonPredicate(v) => v.emit(w),
            Self::JsonParse(v) => v.emit(w),
            Self::JsonScalar(v) => v.emit(w),
            Self::JsonSerialize(v) => v.emit(w),
            Self::JsonObject(v) => v.emit(w),
            Self::JsonArray(v) => v.emit(w),
            Self::JsonArrayQuery(v) => v.emit(w),
            Self::JsonObjectAggregate(v) => v.emit(w),
            Self::JsonArrayAggregate(v) => v.emit(w),
            Self::Xml(v) => v.emit(w),
            Self::XmlSerialize(v) => v.emit(w),
            Self::MultiAssignment(_) => w.invalid(
                "multi-assignment",
                "must occur in a grouped assignment list",
            ),
        }
    }
}
impl SqlEmit for JsonFormat {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.format_type.emit(w)?;
        self.encoding.emit(w)
    }
}
impl SqlEmit for JsonFormatType {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::Default => "",
            Self::Json => " FORMAT JSON",
            Self::Jsonb => " FORMAT JSONB",
        });
        Ok(())
    }
}
impl SqlEmit for JsonEncoding {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::Default => "",
            Self::Utf8 => " ENCODING UTF8",
            Self::Utf16 => " ENCODING UTF16",
            Self::Utf32 => " ENCODING UTF32",
        });
        Ok(())
    }
}
impl SqlEmit for JsonValue {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.raw_expr.emit(w)?;
        if let Some(format) = &self.format {
            format.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for JsonReturning {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if let Some(format) = &self.format {
            format.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for JsonOutput {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if let Some(ty) = &self.type_name {
            w.sql(" RETURNING ");
            ty.emit(w)?;
        }
        if let Some(ret) = &self.returning {
            ret.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for JsonArgument {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.val.emit(w)?;
        w.sql(" AS ");
        w.ident(&self.name)
    }
}
impl SqlEmit for JsonPath {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.string.emit(w)?;
        if !self.name.is_empty() {
            w.sql(" AS ");
            w.ident(&self.name)?;
        }
        Ok(())
    }
}
impl SqlEmit for JsonKeyValue {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.key.emit(w)?;
        w.sql(" : ");
        self.value.emit(w)
    }
}
impl SqlEmit for JsonBehavior {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.btype.emit(w)?;
        if let Some(expr) = &self.expr {
            w.sql(" ");
            expr.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for JsonBehaviorType {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::Null => "NULL",
            Self::Error => "ERROR",
            Self::Empty => "EMPTY",
            Self::True => "TRUE",
            Self::False => "FALSE",
            Self::Unknown => "UNKNOWN",
            Self::EmptyArray => "EMPTY ARRAY",
            Self::EmptyObject => "EMPTY OBJECT",
            Self::Default => "DEFAULT",
        });
        Ok(())
    }
}
impl SqlEmit for JsonWrapper {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::Unspec => "",
            Self::None => " WITHOUT WRAPPER",
            Self::Conditional => " WITH CONDITIONAL WRAPPER",
            Self::Unconditional => " WITH UNCONDITIONAL WRAPPER",
        });
        Ok(())
    }
}
impl SqlEmit for JsonQuotes {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::Unspec => "",
            Self::Keep => " KEEP QUOTES ON SCALAR STRING",
            Self::Omit => " OMIT QUOTES ON SCALAR STRING",
        });
        Ok(())
    }
}
impl JsonFunction {
    fn emit_options(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if !self.passing.is_empty() {
            w.sql(" PASSING ");
            w.list(&self.passing, ", ")?;
        }
        if let Some(output) = &self.output {
            output.emit(w)?;
        }
        self.wrapper.emit(w)?;
        self.quotes.emit(w)?;
        if let Some(empty) = &self.on_empty {
            w.sql(" ");
            empty.emit(w)?;
            w.sql(" ON EMPTY");
        }
        if let Some(error) = &self.on_error {
            w.sql(" ");
            error.emit(w)?;
            w.sql(" ON ERROR");
        }
        Ok(())
    }
}
impl SqlEmit for JsonFunction {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.op.emit(w)?;
        w.sql("(");
        self.context_item.emit(w)?;
        w.sql(", ");
        self.pathspec.emit(w)?;
        self.emit_options(w)?;
        w.sql(")");
        Ok(())
    }
}
impl SqlEmit for JsonExprOp {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::Exists => "JSON_EXISTS",
            Self::Query => "JSON_QUERY",
            Self::Value => "JSON_VALUE",
            Self::Table => "JSON_TABLE",
        });
        Ok(())
    }
}
impl SqlEmit for JsonPredicate {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("(");
        self.expr.emit(w)?;
        if let Some(format) = &self.format {
            format.emit(w)?;
        }
        w.sql(" IS JSON");
        self.item_type.emit(w)?;
        if self.unique_keys {
            w.sql(" WITH UNIQUE KEYS");
        }
        w.sql(")");
        Ok(())
    }
}
impl SqlEmit for JsonValueType {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::Any => "",
            Self::Object => " OBJECT",
            Self::Array => " ARRAY",
            Self::Scalar => " SCALAR",
        });
        Ok(())
    }
}
impl SqlEmit for JsonParse {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("JSON(");
        self.expr.emit(w)?;
        if self.unique_keys {
            w.sql(" WITH UNIQUE KEYS");
        }
        if let Some(output) = &self.output {
            output.emit(w)?;
        }
        w.sql(")");
        Ok(())
    }
}
impl SqlEmit for JsonScalar {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("JSON_SCALAR(");
        self.expr.emit(w)?;
        if let Some(output) = &self.output {
            output.emit(w)?;
        }
        w.sql(")");
        Ok(())
    }
}
impl SqlEmit for JsonSerialize {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("JSON_SERIALIZE(");
        self.expr.emit(w)?;
        if let Some(output) = &self.output {
            output.emit(w)?;
        }
        w.sql(")");
        Ok(())
    }
}
fn null_behavior(w: &mut SqlWriter, absent: bool) {
    w.sql(if absent {
        " ABSENT ON NULL"
    } else {
        " NULL ON NULL"
    });
}
impl SqlEmit for JsonObject {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("JSON_OBJECT(");
        w.list(&self.exprs, ", ")?;
        if !self.exprs.is_empty() {
            null_behavior(w, self.absent_on_null);
        }
        if self.unique {
            w.sql(" WITH UNIQUE KEYS");
        }
        if let Some(output) = &self.output {
            output.emit(w)?;
        }
        w.sql(")");
        Ok(())
    }
}
impl SqlEmit for JsonArray {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("JSON_ARRAY(");
        w.list(&self.exprs, ", ")?;
        if !self.exprs.is_empty() {
            null_behavior(w, self.absent_on_null);
        }
        if let Some(output) = &self.output {
            output.emit(w)?;
        }
        w.sql(")");
        Ok(())
    }
}
impl SqlEmit for JsonArrayQuery {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("JSON_ARRAY(");
        self.query.emit(w)?;
        if let Some(format) = &self.format {
            format.emit(w)?;
        }
        if let Some(output) = &self.output {
            output.emit(w)?;
        }
        w.sql(")");
        Ok(())
    }
}
impl JsonAggregate {
    fn emit_output(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if let Some(output) = &self.output {
            output.emit(w)?;
        }
        Ok(())
    }
    fn emit_tail(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if let Some(filter) = &self.agg_filter {
            w.sql(" FILTER (WHERE ");
            filter.emit(w)?;
            w.sql(")");
        }
        if let Some(over) = &self.over {
            w.sql(" OVER ");
            over.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for JsonObjectAggregate {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("JSON_OBJECTAGG(");
        self.arg.emit(w)?;
        if !self.constructor.agg_order.is_empty() {
            w.sql(" ORDER BY ");
            w.list(&self.constructor.agg_order, ", ")?;
        }
        null_behavior(w, self.absent_on_null);
        if self.unique {
            w.sql(" WITH UNIQUE KEYS");
        }
        self.constructor.emit_output(w)?;
        w.sql(")");
        self.constructor.emit_tail(w)
    }
}
impl SqlEmit for JsonArrayAggregate {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("JSON_ARRAYAGG(");
        self.arg.emit(w)?;
        if !self.constructor.agg_order.is_empty() {
            w.sql(" ORDER BY ");
            w.list(&self.constructor.agg_order, ", ")?;
        }
        null_behavior(w, self.absent_on_null);
        self.constructor.emit_output(w)?;
        w.sql(")");
        self.constructor.emit_tail(w)
    }
}
impl SqlEmit for JsonTable {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.postgres_only("JSON_TABLE")?;
        if self.lateral {
            w.sql("LATERAL ");
        }
        w.sql("JSON_TABLE(");
        self.context_item.emit(w)?;
        w.sql(", ");
        self.pathspec.emit(w)?;
        if !self.passing.is_empty() {
            w.sql(" PASSING ");
            w.list(&self.passing, ", ")?;
        }
        w.sql(" COLUMNS (");
        w.list(&self.columns, ", ")?;
        w.sql(")");
        if let Some(error) = &self.on_error {
            w.sql(" ");
            error.emit(w)?;
            w.sql(" ON ERROR");
        }
        w.sql(")");
        if let Some(alias) = &self.alias {
            alias.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for JsonColumn {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        crate::sem::with_stack(|| self.coltype.emit_column(self, w))
    }
}
impl JsonTableColumnType {
    fn emit_column(&self, v: &JsonColumn, w: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Nested => {
                w.sql("NESTED PATH ");
                let Some(path) = &v.pathspec else {
                    return w.invalid("JSON nested column", "path required");
                };
                path.emit(w)?;
                w.sql(" COLUMNS (");
                w.list(&v.columns, ", ")?;
                w.sql(")");
            }
            Self::ForOrdinality => {
                w.ident(&v.name)?;
                w.sql(" FOR ORDINALITY");
            }
            Self::Regular | Self::Exists | Self::Formatted => {
                w.ident(&v.name)?;
                w.sql(" ");
                let Some(ty) = &v.type_name else {
                    return w.invalid("JSON column", "type required");
                };
                ty.emit(w)?;
                if let Some(format) = &v.format {
                    format.emit(w)?;
                }
                if matches!(self, Self::Exists) {
                    w.sql(" EXISTS");
                }
                if let Some(path) = &v.pathspec {
                    w.sql(" PATH ");
                    path.emit(w)?;
                }
                if !matches!(self, Self::Exists) {
                    v.wrapper.emit(w)?;
                    v.quotes.emit(w)?;
                }
                if let Some(empty) = &v.on_empty {
                    w.sql(" ");
                    empty.emit(w)?;
                    w.sql(" ON EMPTY");
                }
                if let Some(error) = &v.on_error {
                    w.sql(" ");
                    error.emit(w)?;
                    w.sql(" ON ERROR");
                }
            }
        }
        Ok(())
    }
}
impl SqlEmit for XmlOptionType {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::Document => "DOCUMENT",
            Self::Content => "CONTENT",
        });
        Ok(())
    }
}
impl SqlEmit for XmlSerialize {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("XMLSERIALIZE(");
        self.xmloption.emit(w)?;
        w.sql(" ");
        self.expr.emit(w)?;
        w.sql(" AS ");
        self.type_name.emit(w)?;
        w.sql(if self.indent {
            " INDENT)"
        } else {
            " NO INDENT)"
        });
        Ok(())
    }
}
impl SqlEmit for Xml {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.op.emit_with(self, w)
    }
}
impl XmlExprOp {
    fn emit_with(&self, v: &Xml, w: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Xmlconcat => {
                w.sql("XMLCONCAT(");
                w.list(&v.args, ", ")?;
                w.sql(")");
            }
            Self::Xmlelement => {
                w.sql("XMLELEMENT(NAME ");
                w.ident(&v.name)?;
                if !v.named_args.is_empty() {
                    w.sql(", XMLATTRIBUTES(");
                    w.list(&v.named_args, ", ")?;
                    w.sql(")");
                }
                if !v.args.is_empty() {
                    w.sql(", ");
                    w.list(&v.args, ", ")?;
                }
                w.sql(")");
            }
            Self::Xmlforest => {
                w.sql("XMLFOREST(");
                w.list(&v.named_args, ", ")?;
                if !v.args.is_empty() {
                    if !v.named_args.is_empty() {
                        w.sql(", ");
                    }
                    w.list(&v.args, ", ")?;
                }
                w.sql(")");
            }
            Self::Xmlparse => {
                w.sql("XMLPARSE(");
                v.xmloption.emit(w)?;
                w.sql(" ");
                let Some(expr) = v.args.first() else {
                    return w.invalid("XMLPARSE", "missing expression");
                };
                expr.emit(w)?;
                let Some(preserve) = v.args.get(1).and_then(crate::ast::Expr::boolean_literal)
                else {
                    return w.invalid("XMLPARSE", "whitespace choice must be a boolean literal");
                };
                w.sql(if preserve {
                    " PRESERVE WHITESPACE)"
                } else {
                    " STRIP WHITESPACE)"
                });
            }
            Self::Xmlpi => {
                w.sql("XMLPI(NAME ");
                w.ident(&v.name)?;
                if !v.args.is_empty() {
                    w.sql(", ");
                    w.list(&v.args, ", ")?;
                }
                w.sql(")");
            }
            Self::Xmlroot => {
                let [expr, version, standalone] = v.args.as_slice() else {
                    return w.invalid("XMLROOT", "expected three arguments");
                };
                w.sql("XMLROOT(");
                expr.emit(w)?;
                w.sql(", VERSION ");
                if version.is_null_literal() {
                    w.sql("NO VALUE");
                } else {
                    version.emit(w)?;
                }
                let kind = standalone
                    .integer_literal()
                    .ok_or_else(|| w.invalid_error("XMLROOT", "invalid standalone option"))?;
                w.sql(match kind {
                    0 => ", STANDALONE YES",
                    1 => ", STANDALONE NO",
                    2 => ", STANDALONE NO VALUE",
                    3 => "",
                    _ => return w.invalid("XMLROOT", "invalid standalone option"),
                });
                w.sql(")");
            }
            Self::Document => {
                let Some(expr) = v.args.first() else {
                    return w.invalid("IS DOCUMENT", "missing expression");
                };
                w.sql("(");
                expr.emit(w)?;
                w.sql(" IS DOCUMENT)");
            }
            Self::Xmlserialize => {
                return w.invalid("XMLSERIALIZE", "expected the syntax XmlSerialize node");
            }
        }
        Ok(())
    }
}
impl SqlEmit for XmlTable {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.postgres_only("XMLTABLE")?;
        if self.lateral {
            w.sql("LATERAL ");
        }
        w.sql("XMLTABLE(");
        if !self.namespaces.is_empty() {
            w.sql("XMLNAMESPACES(");
            for (i, namespace) in self.namespaces.iter().enumerate() {
                if i > 0 {
                    w.sql(", ");
                }
                namespace.emit_namespace(w)?;
            }
            w.sql("), ");
        }
        self.rowexpr.emit(w)?;
        if let Some(doc) = &self.docexpr {
            w.sql(" PASSING ");
            doc.emit(w)?;
        }
        w.sql(" COLUMNS ");
        w.list(&self.columns, ", ")?;
        w.sql(")");
        if let Some(alias) = &self.alias {
            alias.emit(w)?;
        }
        Ok(())
    }
}
impl SqlEmit for XmlColumn {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.ident(&self.colname)?;
        if self.for_ordinality {
            w.sql(" FOR ORDINALITY");
        } else {
            let Some(ty) = &self.type_name else {
                return w.invalid("XML column", "type required");
            };
            w.sql(" ");
            ty.emit(w)?;
            if let Some(path) = &self.colexpr {
                w.sql(" PATH ");
                path.emit(w)?;
            }
            if let Some(default) = &self.coldefexpr {
                w.sql(" DEFAULT ");
                default.emit(w)?;
            }
            if self.is_not_null {
                w.sql(" NOT NULL");
            }
        }
        Ok(())
    }
}
impl SqlEmit for Merge {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.postgres_only("MERGE")?;
        self.with_clause.emit(w)?;
        w.sql("MERGE INTO ");
        self.relation.emit(w)?;
        w.sql(" USING ");
        self.source_relation.emit(w)?;
        w.sql(" ON ");
        self.join_condition.emit(w)?;
        for clause in &self.merge_when_clauses {
            clause.emit(w)?;
        }
        if !self.returning_list.is_empty() {
            w.sql(" RETURNING ");
            w.list(&self.returning_list, ", ")?;
        }
        Ok(())
    }
}
impl SqlEmit for MergeWhen {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(" WHEN ");
        self.match_kind.emit(w)?;
        if let Some(condition) = &self.condition {
            w.sql(" AND ");
            condition.emit(w)?;
        }
        w.sql(" THEN ");
        self.command_type.emit_action(self, w)
    }
}
impl SqlEmit for MergeMatchKind {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql(match self {
            Self::Matched => "MATCHED",
            Self::NotMatchedBySource => "NOT MATCHED BY SOURCE",
            Self::NotMatchedByTarget => "NOT MATCHED BY TARGET",
        });
        Ok(())
    }
}
impl CmdType {
    fn emit_action(&self, v: &MergeWhen, w: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Nothing => w.sql("DO NOTHING"),
            Self::Delete => w.sql("DELETE"),
            Self::Update => {
                w.sql("UPDATE SET ");
                Assignment::emit_list(w, &v.target_list)?;
            }
            Self::Insert => {
                w.sql("INSERT");
                if !v.target_list.is_empty() {
                    w.sql("(");
                    for (i, a) in v.target_list.iter().enumerate() {
                        if i > 0 {
                            w.sql(", ");
                        }
                        a.emit_target(w)?;
                    }
                    w.sql(")");
                }
                v.overriding.emit(w)?;
                if v.values.is_empty() {
                    w.sql(" DEFAULT VALUES");
                } else {
                    w.sql(" VALUES (");
                    w.list(&v.values, ", ")?;
                    w.sql(")");
                }
            }
            _ => return w.invalid("MERGE action", "unsupported command in WHEN"),
        };
        Ok(())
    }
}

impl crate::ast::Target {
    fn emit_namespace(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if let Some(alias) = &self.alias {
            self.expression.emit(w)?;
            w.sql(" AS ");
            w.ident(alias)?;
        } else {
            w.sql("DEFAULT ");
            self.expression.emit(w)?;
        }
        Ok(())
    }
}
