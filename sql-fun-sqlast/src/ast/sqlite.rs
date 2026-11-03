mod ddl;
mod dml;
mod query;

use crate::{
    ast::{AlterRelation, CreateTrigger, DropTrigger, RaiseAction, Table},
    sem::with_stack,
};

use crate::ast::{
    Access, BooleanExpr, BooleanOperator, Call, CallKind, Case, CaseArm, Cast, Collate, Delete,
    DropTable, Expr, InTable, Indirection, Insert, Literal, Name, NullTest, OperatorExpr,
    OperatorKind, Order, Parameter, Query, Raise, SqlValue, SqliteTypeNamePart,
    SqliteTypeNameTrivia, StatementNode, Subquery, SubqueryKind, TryFromList, TryFromOpt, TypeName,
    Update, View, ViewCheckOption, Window,
};
#[cfg(feature = "workaround_turso_9791")]
#[cfg(feature = "sqlite")]
use crate::ast::{SqliteTypeModifier, SqliteTypeModifiers};
#[cfg(feature = "sqlite")]
use crate::{sem::AnalysisError, syn::sqlite as syn};

impl From<&syn::QualifiedName> for Name {
    fn from(value: &syn::QualifiedName) -> Self {
        let mut parts = Vec::new();
        if let Some(database) = value.db_name() {
            parts.push(database.value().to_owned());
        }
        parts.push(value.name().value().to_owned());
        Self::from_parts(&parts)
    }
}

impl TryFrom<&syn::Stmt> for StatementNode {
    type Error = AnalysisError;

    fn try_from(value: &syn::Stmt) -> Result<Self, Self::Error> {
        match value {
            syn::Stmt::Select(query) => Ok(Self::query(Query::try_from(query)?)),
            syn::Stmt::Insert(insert) => Ok(Self::insert(Insert::try_from(insert)?)),
            syn::Stmt::Update(update) => Ok(Self::update(Update::try_from(update)?)),
            syn::Stmt::Delete(delete) => Ok(Self::delete(Delete::try_from(delete)?)),
            syn::Stmt::CreateTable(table) => Ok(Self::table(Table::try_from(table)?)),
            syn::Stmt::AlterTable(table) => {
                Ok(Self::alter_relation(AlterRelation::try_from(table)?))
            }
            syn::Stmt::DropTrigger(trigger) => {
                Ok(Self::drop_trigger(DropTrigger::try_from(trigger)?))
            }
            syn::Stmt::CreateTrigger(trigger) => {
                Ok(Self::create_trigger(CreateTrigger::try_from(trigger)?))
            }
            syn::Stmt::CreateView(view) => Ok(Self::view(View::try_from(view)?)),
            syn::Stmt::DropTable(drop_table) => Ok(Self::drop_table(DropTable::from(drop_table))),
            _ => AnalysisError::raise_unexpected_input("top-level SQLite statement"),
        }
    }
}

impl From<&syn::DropTableStatement> for DropTable {
    fn from(value: &syn::DropTableStatement) -> Self {
        Self::new(
            vec![Name::from(value.name())],
            *value.if_exists(),
            false,
            false,
        )
    }
}

impl TryFrom<&syn::CreateView> for View {
    type Error = AnalysisError;
    fn try_from(value: &syn::CreateView) -> Result<Self, Self::Error> {
        Ok(Self::new(
            Name::from(value.view_name()),
            value
                .columns()
                .iter()
                .map(|column| column.column_name().value().to_owned())
                .collect(),
            Query::try_from(value.select())?,
            false,
            Vec::new(),
            ViewCheckOption::None,
            *value.temporary(),
            *value.if_not_exists(),
            false,
        ))
    }
}

impl TryFrom<&syn::Expr> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::Expr) -> Result<Self, Self::Error> {
        with_stack(|| Self::convert_sqlite(value))
    }
}

impl Expr {
    #[inline(never)]
    fn convert_sqlite(value: &syn::Expr) -> Result<Self, AnalysisError> {
        Ok(match value {
            syn::Expr::Literal(value) => Self::from(value),
            syn::Expr::Id(name) | syn::Expr::Name(name) => Self::from(name),
            syn::Expr::Qualified(name) => Self::try_from(name)?,
            syn::Expr::DoublyQualified(name) => Self::try_from(name)?,
            syn::Expr::Parenthesized(value) => Self::try_from(value)?,
            syn::Expr::Unary(value) => Self::try_from(value)?,
            syn::Expr::Binary(value) => Self::try_from(value)?,
            syn::Expr::Between(value) => Self::try_from(value)?,
            syn::Expr::IsNull(value) => Self::try_from(value)?,
            syn::Expr::NotNull(value) => Self::try_from(value)?,
            syn::Expr::Case(value) => Self::try_from(value)?,
            syn::Expr::Cast(value) => Self::try_from(value)?,
            syn::Expr::Collate(value) => Self::try_from(value)?,
            syn::Expr::FunctionCall(value) => Self::call(Call::try_from(value)?),
            syn::Expr::FunctionCallStar(value) => Self::try_from(value)?,
            syn::Expr::Subquery(query) => Self::scalar_subquery(query)?,
            syn::Expr::Exists(query) => Self::exists_subquery(query)?,
            syn::Expr::InList(value) => Self::try_from(value)?,
            syn::Expr::InSelect(value) => Self::try_from(value)?,
            syn::Expr::InTable(value) => Self::try_from(value)?,
            syn::Expr::Like(value) => Self::try_from(value)?,
            syn::Expr::Variable(value) => Self::from(value),
            syn::Expr::Raise(value) => Self::try_from(value)?,
            syn::Expr::Array(value) => Self::try_from(value)?,
            syn::Expr::Subscript(value) => Self::try_from(value)?,
            syn::Expr::FieldAccess(value) => Self::try_from(value)?,
            syn::Expr::Default => Self::default_value(),
            // These are Turso planner outputs, never context-free SQL parser inputs.
            syn::Expr::Column(_)
            | syn::Expr::RowId(_)
            | syn::Expr::Register(_)
            | syn::Expr::SubqueryResult(_) => {
                return AnalysisError::raise_unexpected_input(
                    "resolved Turso expression in syntax input",
                );
            }
        })
    }
}

impl TryFrom<&Box<syn::Expr>> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &Box<syn::Expr>) -> Result<Self, Self::Error> {
        Self::try_from(value.as_ref())
    }
}

impl Expr {
    fn scalar_subquery(value: &syn::Select) -> Result<Self, AnalysisError> {
        Ok(Self::subquery(Subquery::scalar(value)?))
    }

    fn exists_subquery(value: &syn::Select) -> Result<Self, AnalysisError> {
        Ok(Self::subquery(Subquery::exists(value)?))
    }

    const fn default_value() -> Self {
        Self::Default
    }

    fn ordinary_operator(name: &str, left: Option<Self>, right: Option<Self>) -> Self {
        Self::operator(OperatorExpr::ordinary(name, left, right))
    }

    fn operator(operator: OperatorExpr) -> Self {
        Self::Operator(Box::new(operator))
    }

    fn boolean(boolean: BooleanExpr) -> Self {
        Self::Boolean(Box::new(boolean))
    }

    fn call(call: Call) -> Self {
        Self::Call(Box::new(call))
    }

    fn subquery(subquery: Subquery) -> Self {
        Self::Subquery(Box::new(subquery))
    }

    fn case(case: Case) -> Self {
        Self::Case(Box::new(case))
    }

    fn in_table(in_table: InTable) -> Self {
        Self::InTable(Box::new(in_table))
    }

    fn raise(raise: Raise) -> Self {
        Self::Raise(Box::new(raise))
    }

    fn indirection(indirection: Indirection) -> Self {
        Self::Indirection(Box::new(indirection))
    }
}

impl From<&syn::Name> for Expr {
    fn from(value: &syn::Name) -> Self {
        Self::Column(Name::local(value.value()))
    }
}

impl TryFrom<&syn::Qualified> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::Qualified) -> Result<Self, Self::Error> {
        Ok(Self::Column(Name::from_parts(&[
            value.table().value().to_owned(),
            value.column().value().to_owned(),
        ])))
    }
}

impl TryFrom<&syn::ExprDoublyQualified> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprDoublyQualified) -> Result<Self, Self::Error> {
        Ok(Self::Column(Name::from_parts(&[
            value.schema().value().to_owned(),
            value.table().value().to_owned(),
            value.column().value().to_owned(),
        ])))
    }
}

impl TryFrom<&syn::ExprCase> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprCase) -> Result<Self, Self::Error> {
        Ok(Self::case(Case::try_from(value)?))
    }
}

impl TryFrom<&syn::ExprCase> for Case {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprCase) -> Result<Self, Self::Error> {
        let mut arms = Vec::new();
        for arm in value.when_then_pairs() {
            arms.push(CaseArm {
                condition: Expr::try_from(arm.when())?,
                result: Expr::try_from(arm.then())?,
            });
        }
        Ok(Self {
            argument: Expr::try_from_opt(value.base().as_deref())?,
            arms,
            otherwise: Expr::try_from_opt(value.else_expr().as_deref())?,
        })
    }
}

impl TryFrom<&syn::ExprFunctionCallStar> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprFunctionCallStar) -> Result<Self, Self::Error> {
        Ok(Self::call(Call::try_from(value)?))
    }
}

impl TryFrom<&syn::ExprFunctionCallStar> for Call {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprFunctionCallStar) -> Result<Self, Self::Error> {
        let mut call = Self::base(value.name().value());
        call.star = true;
        call.apply_tail(value.filter_over())?;
        Ok(call)
    }
}

impl TryFrom<&syn::ExprInList> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprInList) -> Result<Self, Self::Error> {
        Ok(Self::operator(OperatorExpr::try_from(value)?))
    }
}

impl TryFrom<&syn::ExprInList> for OperatorExpr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprInList) -> Result<Self, Self::Error> {
        Ok(Self {
            name: Name::default(),
            kind: if *value.not() {
                OperatorKind::not_in()
            } else {
                OperatorKind::in_list()
            },
            left: Some(Expr::try_from(value.lhs())?),
            right: Some(Expr::List(Expr::try_from_list(value.rhs())?)),
            escape: None,
        })
    }
}

impl TryFrom<&syn::ExprInSelect> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprInSelect) -> Result<Self, Self::Error> {
        let expr = Self::subquery(Subquery::try_from(value)?);
        Ok(if *value.not() {
            Self::boolean(BooleanExpr::not(expr))
        } else {
            expr
        })
    }
}

impl TryFrom<&syn::ExprInSelect> for Subquery {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprInSelect) -> Result<Self, Self::Error> {
        Ok(Self {
            query: Query::try_from(value.rhs())?,
            kind: SubqueryKind::any(),
            test: Some(Expr::try_from(value.lhs())?),
            operator: Name::local("="),
        })
    }
}

impl Subquery {
    fn scalar(value: &syn::Select) -> Result<Self, AnalysisError> {
        Ok(Self {
            query: Query::try_from(value)?,
            kind: SubqueryKind::scalar(),
            test: None,
            operator: Name::default(),
        })
    }

    fn exists(value: &syn::Select) -> Result<Self, AnalysisError> {
        Ok(Self {
            query: Query::try_from(value)?,
            kind: SubqueryKind::exists(),
            test: None,
            operator: Name::default(),
        })
    }
}

impl TryFrom<&syn::ExprInTable> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprInTable) -> Result<Self, Self::Error> {
        Ok(Self::in_table(InTable::try_from(value)?))
    }
}

impl TryFrom<&syn::ExprInTable> for InTable {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprInTable) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from(value.lhs())?,
            negated: *value.not(),
            table: Name::from(value.rhs()),
            arguments: Expr::try_from_list(value.args())?,
        })
    }
}

impl TryFrom<&syn::ExprLike> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprLike) -> Result<Self, Self::Error> {
        Ok(Self::operator(OperatorExpr::try_from(value)?))
    }
}

impl TryFrom<&syn::ExprLike> for OperatorExpr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprLike) -> Result<Self, Self::Error> {
        Ok(Self {
            name: Name::local(match value.op() {
                syn::LikeOperator::Like => "like",
                syn::LikeOperator::Glob => "glob",
                syn::LikeOperator::Match => "match",
                syn::LikeOperator::Regexp => "regexp",
            }),
            kind: if *value.not() {
                OperatorKind::not_like()
            } else {
                OperatorKind::like()
            },
            left: Some(Expr::try_from(value.lhs())?),
            right: Some(Expr::try_from(value.rhs())?),
            escape: Expr::try_from_opt(value.escape().as_deref())?,
        })
    }
}

#[cfg(test)]
mod test_pattern_classification {
    #[cfg(feature = "sqlite")]
    use crate::ast::sqlite::{OperatorExpr, OperatorKind, syn};

    #[rstest::rstest]
    #[case(syn::LikeOperator::Like, "like")]
    #[case(syn::LikeOperator::Glob, "glob")]
    #[case(syn::LikeOperator::Match, "match")]
    #[case(syn::LikeOperator::Regexp, "regexp")]
    fn preserves_identity_and_negation(
        #[case] operator: syn::LikeOperator,
        #[case] name: &str,
        #[values(false, true)] negated: bool,
    ) -> testresult::TestResult {
        let operand = syn::Expr::literal(syn::Literal::from(::turso_parser::ast::Literal::Null));
        let syntax = syn::ExprLike::new(operand.clone(), negated, operator, operand, None);
        let common = OperatorExpr::try_from(&syntax)?;
        assert_eq!(common.name.0.as_slice(), &[name]);
        assert_eq!(matches!(common.kind, OperatorKind::NotLike), negated);
        assert!(common.left.is_some());
        assert!(common.right.is_some());
        Ok(())
    }
}

impl From<&syn::Variable> for Expr {
    fn from(value: &syn::Variable) -> Self {
        Self::Parameter(Parameter {
            position: value.index().get(),
            name: value.name().clone(),
            type_name: value.col_type().clone(),
        })
    }
}

impl TryFrom<&syn::ExprRaise> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprRaise) -> Result<Self, Self::Error> {
        Ok(Self::raise(Raise::try_from(value)?))
    }
}

impl TryFrom<&syn::ExprRaise> for Raise {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprRaise) -> Result<Self, Self::Error> {
        Ok(Self {
            action: RaiseAction::from(value.resolve_type()),
            message: Expr::try_from_opt(value.expr().as_deref())?,
        })
    }
}

impl TryFrom<&syn::Array> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::Array) -> Result<Self, Self::Error> {
        Ok(Self::Array(Self::try_from_list(value.syntax_elements())?))
    }
}

impl TryFrom<&syn::Subscript> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::Subscript) -> Result<Self, Self::Error> {
        Ok(Self::indirection(Indirection::try_from(value)?))
    }
}

impl TryFrom<&syn::Subscript> for Indirection {
    type Error = AnalysisError;

    fn try_from(value: &syn::Subscript) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from(value.syntax_base())?,
            items: vec![Access::try_from(value.syntax_index())?],
        })
    }
}

impl TryFrom<&syn::FieldAccess> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::FieldAccess) -> Result<Self, Self::Error> {
        Ok(Self::indirection(Indirection::try_from(value)?))
    }
}

impl TryFrom<&syn::FieldAccess> for Indirection {
    type Error = AnalysisError;

    fn try_from(value: &syn::FieldAccess) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from(value.base())?,
            items: vec![Access::from(value.field())],
        })
    }
}

impl From<&syn::Literal> for Expr {
    fn from(value: &syn::Literal) -> Self {
        match value {
            syn::Literal::CurrentDate(_) => Self::SqlValue(SqlValue {
                name: "current_date".to_owned(),
                precision: None,
            }),
            syn::Literal::CurrentTime(_) => Self::SqlValue(SqlValue {
                name: "current_time".to_owned(),
                precision: None,
            }),
            syn::Literal::CurrentTimestamp(_) => Self::SqlValue(SqlValue {
                name: "current_timestamp".to_owned(),
                precision: None,
            }),
            literal => Self::Literal(Literal::from(literal)),
        }
    }
}

impl From<&syn::Literal> for Literal {
    fn from(value: &syn::Literal) -> Self {
        match value {
            syn::Literal::Numeric(value) => Self::from(value),
            syn::Literal::String(value) => Self::from(value),
            syn::Literal::Null(value) => Self::from(value),
            syn::Literal::True(value) | syn::Literal::False(value) => Self::from(value),
            syn::Literal::Blob(value) => Self::from(value),
            syn::Literal::Keyword(value) => Self::from(value),
            syn::Literal::CurrentDate(_)
            | syn::Literal::CurrentTime(_)
            | syn::Literal::CurrentTimestamp(_) => Self::timestamp(),
        }
    }
}

impl From<&syn::NumericLiteral> for Literal {
    fn from(value: &syn::NumericLiteral) -> Self {
        match value.value_text().parse::<i32>() {
            Ok(value) => Self::Integer(value),
            Err(_) => Self::Numeric(value.value_text().to_owned()),
        }
    }
}

impl From<&syn::StringLiteral> for Literal {
    fn from(value: &syn::StringLiteral) -> Self {
        let value = value.value();
        Self::String(
            if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
                value[1..value.len() - 1].replace("''", "'")
            } else {
                value.to_owned()
            },
        )
    }
}

impl From<&syn::NullLiteral> for Literal {
    fn from(_: &syn::NullLiteral) -> Self {
        Self::Null
    }
}

impl From<&syn::BooleanLiteral> for Literal {
    fn from(value: &syn::BooleanLiteral) -> Self {
        Self::Boolean(value.value())
    }
}

impl From<&syn::BlobLiteral> for Literal {
    fn from(value: &syn::BlobLiteral) -> Self {
        Self::Blob(value.value().to_owned())
    }
}

impl From<&syn::KeywordLiteral> for Literal {
    fn from(value: &syn::KeywordLiteral) -> Self {
        Self::Keyword(value.value().to_owned())
    }
}

impl Literal {
    fn timestamp() -> Self {
        unreachable!("SQL values are converted separately")
    }
}

impl From<&syn::ResolveType> for RaiseAction {
    fn from(value: &syn::ResolveType) -> Self {
        match value {
            syn::ResolveType::Rollback => Self::Rollback,
            syn::ResolveType::Abort => Self::Abort,
            syn::ResolveType::Fail => Self::Fail,
            syn::ResolveType::Ignore => Self::Ignore,
            syn::ResolveType::Replace => Self::Replace,
        }
    }
}

impl TypeName {
    fn empty() -> Self {
        Self {
            name: Name::default(),
            modifiers: Vec::new(),
            array_bounds: Vec::new(),
            set_of: false,
            percent_type: false,
            sqlite_name_snapshot: None,
            sqlite_parts: Vec::new(),
            sqlite_modifiers: None,
            location: None,
        }
    }
}

impl TryFrom<&syn::Type> for TypeName {
    type Error = AnalysisError;

    fn try_from(value: &syn::Type) -> Result<Self, Self::Error> {
        let modifiers = match value.size() {
            Some(syn::TypeSize::MaxSize(size)) => vec![Expr::try_from(size)?],
            Some(syn::TypeSize::TypeSize(size, scale)) => {
                vec![Expr::try_from(size)?, Expr::try_from(scale)?]
            }
            None => Vec::new(),
        };
        let raw_name = value.name();
        let (decoded_name, sqlite_parts) = decode_sqlite_type_name_parts(raw_name)
            .unwrap_or_else(|_| (raw_name.to_owned(), Vec::new()));
        let name = Name::local(&decoded_name);
        Ok(Self {
            sqlite_name_snapshot: Some(name.clone()),
            name,
            sqlite_parts,
            modifiers,
            ..Self::empty()
        })
    }
}

#[cfg(feature = "workaround_turso_9791")]
impl TypeName {
    pub(in crate::ast) fn matches_sqlite_frontend_name(
        &self,
        frontend_name: &str,
    ) -> Result<bool, AnalysisError> {
        match decode_sqlite_type_name_parts(frontend_name) {
            Ok((decoded_name, _)) => Ok(matches!(
                self.name.0.as_slice(),
                [name] if name == &decoded_name
            )),
            Err(_) => Ok(matches!(
                self.name.0.as_slice(),
                [name] if name == frontend_name
            )),
        }
    }

    pub(in crate::ast) fn retain_sqlite_syntax(
        &mut self,
        full_type_text: &str,
    ) -> Result<(), AnalysisError> {
        let open = sqlite_type_modifier_opening(full_type_text)?;
        let (name_text, mut modifiers) = if let Some(open) = open {
            (
                &full_type_text[..open],
                Some(parse_sqlite_type_modifiers(&full_type_text[open..])?),
            )
        } else {
            (full_type_text, None)
        };
        let (decoded_name, parts) = decode_sqlite_type_name_parts(name_text)?;
        if let Some(syntax) = modifiers.as_mut() {
            if syntax.values.len() != self.modifiers.len() {
                return AnalysisError::raise_unexpected_input("SQLite type modifier association");
            }
            syntax.ast_spellings.clear();
            for modifier in &self.modifiers {
                let Some(spelling) = modifier.sqlite_type_modifier_spelling() else {
                    return AnalysisError::raise_unexpected_input("SQLite type modifier");
                };
                syntax.ast_spellings.push(spelling);
            }
        }
        self.name = Name::local(&decoded_name);
        self.sqlite_name_snapshot = Some(self.name.clone());
        self.sqlite_parts = parts;
        self.sqlite_modifiers = modifiers;
        Ok(())
    }
}

fn decode_sqlite_type_name_parts(
    raw: &str,
) -> Result<(String, Vec<SqliteTypeNamePart>), AnalysisError> {
    let bytes = raw.as_bytes();
    let mut cursor = 0;
    let mut decoded = String::new();
    let mut parts: Vec<SqliteTypeNamePart> = Vec::new();

    while cursor < bytes.len() {
        let leading_trivia = sqlite_type_name_trivia(raw, &mut cursor)?;
        if cursor == bytes.len() {
            if let Some(last) = parts.last_mut() {
                last.trailing_trivia = leading_trivia;
            }
            break;
        }

        let quote = match bytes[cursor] {
            b'"' => Some('"'),
            b'\'' => Some('\''),
            b'`' => Some('`'),
            b'[' => Some('['),
            _ => None,
        };
        let name = if let Some(quote) = quote {
            let start = cursor;
            cursor += 1;
            let mut closed = false;
            while cursor < bytes.len() {
                if quote == '[' {
                    if bytes[cursor] == b']' {
                        cursor += 1;
                        closed = true;
                        break;
                    }
                } else if bytes[cursor] == quote as u8 {
                    if bytes.get(cursor + 1) == Some(&(quote as u8)) {
                        cursor += 2;
                        continue;
                    }
                    cursor += 1;
                    closed = true;
                    break;
                }
                cursor += 1;
            }
            if !closed {
                return AnalysisError::raise_unexpected_input("SQLite type name quoting");
            }
            ::turso_parser::ast::Name::from_string(&raw[start..cursor])
                .as_str()
                .to_owned()
        } else {
            let start = cursor;
            while cursor < bytes.len()
                && !bytes[cursor].is_ascii_whitespace()
                && !matches!(bytes[cursor], b'"' | b'\'' | b'`' | b'[')
                && bytes.get(cursor..cursor + 2) != Some(b"/*")
                && bytes.get(cursor..cursor + 2) != Some(b"--")
            {
                cursor += 1;
            }
            raw[start..cursor].to_owned()
        };

        for trivia in &leading_trivia {
            if let SqliteTypeNameTrivia::Whitespace(whitespace) = trivia {
                decoded.push_str(whitespace);
            }
        }
        decoded.push_str(&name);
        parts.push(SqliteTypeNamePart {
            name,
            quote,
            leading_trivia,
            trailing_trivia: Vec::new(),
        });
    }

    if parts.is_empty() {
        return AnalysisError::raise_unexpected_input("empty SQLite type name");
    }
    Ok((decoded, parts))
}

#[cfg(feature = "workaround_turso_9791")]
fn sqlite_type_modifier_opening(raw: &str) -> Result<Option<usize>, AnalysisError> {
    let bytes = raw.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' | b'\'' | b'`' => {
                let quote = bytes[cursor];
                cursor += 1;
                let mut closed = false;
                while cursor < bytes.len() {
                    if bytes[cursor] == quote {
                        if bytes.get(cursor + 1) == Some(&quote) {
                            cursor += 2;
                            continue;
                        }
                        cursor += 1;
                        closed = true;
                        break;
                    }
                    cursor += 1;
                }
                if !closed {
                    return AnalysisError::raise_unexpected_input("SQLite type name quoting");
                }
            }
            b'[' => {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor] != b']' {
                    cursor += 1;
                }
                if cursor == bytes.len() {
                    return AnalysisError::raise_unexpected_input("SQLite type name quoting");
                }
                cursor += 1;
            }
            b'/' if bytes.get(cursor + 1) == Some(&b'*') => {
                cursor += 2;
                while cursor + 1 < bytes.len() && bytes.get(cursor..cursor + 2) != Some(b"*/") {
                    cursor += 1;
                }
                if cursor + 1 >= bytes.len() {
                    return AnalysisError::raise_unexpected_input("SQLite type name comment");
                }
                cursor += 2;
            }
            b'-' if bytes.get(cursor + 1) == Some(&b'-') => {
                cursor += 2;
                while cursor < bytes.len() && !matches!(bytes[cursor], b'\r' | b'\n') {
                    cursor += 1;
                }
            }
            b'(' => return Ok(Some(cursor)),
            _ => cursor += 1,
        }
    }
    Ok(None)
}

#[cfg(feature = "workaround_turso_9791")]
fn parse_sqlite_type_modifiers(raw: &str) -> Result<SqliteTypeModifiers, AnalysisError> {
    let bytes = raw.as_bytes();
    if bytes.first() != Some(&b'(') {
        return AnalysisError::raise_unexpected_input("SQLite type modifiers");
    }
    let mut cursor = 1;
    let mut values = Vec::new();
    loop {
        let leading_trivia = sqlite_type_name_trivia(raw, &mut cursor)?;
        let sign_start = cursor;
        if cursor < bytes.len() && matches!(bytes[cursor], b'+' | b'-') {
            cursor += 1;
        }
        let sign = &raw[sign_start..cursor];
        let sign_trivia = if sign.is_empty() {
            Vec::new()
        } else {
            sqlite_type_name_trivia(raw, &mut cursor)?
        };
        let start = cursor;
        while cursor < bytes.len()
            && !bytes[cursor].is_ascii_whitespace()
            && !matches!(bytes[cursor], b',' | b')')
            && bytes.get(cursor..cursor + 2) != Some(b"/*")
            && bytes.get(cursor..cursor + 2) != Some(b"--")
        {
            cursor += 1;
        }
        if start == cursor {
            return AnalysisError::raise_unexpected_input("SQLite numeric type modifier");
        }
        let spelling = format!("{sign}{}", &raw[start..cursor]);
        if !valid_sqlite_numeric(&spelling) {
            return AnalysisError::raise_unexpected_input("SQLite numeric type modifier");
        }
        let mut trailing_trivia = sign_trivia;
        trailing_trivia.extend(sqlite_type_name_trivia(raw, &mut cursor)?);
        if cursor >= bytes.len() {
            return AnalysisError::raise_unexpected_input("SQLite type modifiers");
        }
        match bytes[cursor] {
            b',' => {
                values.push(SqliteTypeModifier {
                    leading_trivia,
                    spelling,
                    trailing_trivia,
                });
                cursor += 1;
            }
            b')' => {
                values.push(SqliteTypeModifier {
                    leading_trivia,
                    spelling,
                    trailing_trivia,
                });
                cursor += 1;
                let trailing_trivia = sqlite_type_name_trivia(raw, &mut cursor)?;
                if cursor != bytes.len() || values.is_empty() || values.len() > 2 {
                    return AnalysisError::raise_unexpected_input("SQLite type modifiers");
                }
                return Ok(SqliteTypeModifiers {
                    values,
                    ast_spellings: Vec::new(),
                    trailing_trivia,
                });
            }
            _ => return AnalysisError::raise_unexpected_input("SQLite type modifiers"),
        }
    }
}

fn valid_sqlite_numeric(value: &str) -> bool {
    fn strip_sign(value: &str) -> &str {
        if let Some(unsigned) = value.strip_prefix('+') {
            unsigned
        } else if let Some(unsigned) = value.strip_prefix('-') {
            unsigned
        } else {
            value
        }
    }

    fn valid_digit_run(value: &str, is_digit: impl Fn(u8) -> bool) -> bool {
        let bytes = value.as_bytes();
        !bytes.is_empty()
            && bytes.iter().enumerate().all(|(index, byte)| {
                if *byte == b'_' {
                    index > 0
                        && index + 1 < bytes.len()
                        && is_digit(bytes[index - 1])
                        && is_digit(bytes[index + 1])
                } else {
                    is_digit(*byte)
                }
            })
    }

    let value = strip_sign(value);
    if value.is_empty() {
        return false;
    }
    let hexadecimal = if let Some(hexadecimal) = value.strip_prefix("0x") {
        Some(hexadecimal)
    } else {
        value.strip_prefix("0X")
    };
    if let Some(hexadecimal) = hexadecimal {
        return valid_digit_run(hexadecimal, |byte| byte.is_ascii_hexdigit());
    }

    let exponent_at = value.char_indices().find_map(|(index, character)| {
        if matches!(character, 'e' | 'E') {
            Some(index)
        } else {
            None
        }
    });
    if exponent_at.is_some_and(|index| {
        value[index + 1..]
            .bytes()
            .any(|byte| matches!(byte, b'e' | b'E'))
    }) {
        return false;
    }
    let (mantissa, exponent) = exponent_at.map_or((value, None), |index| {
        (&value[..index], Some(&value[index + 1..]))
    });
    if let Some(exponent) = exponent {
        let exponent = strip_sign(exponent);
        if !valid_digit_run(exponent, |byte| byte.is_ascii_digit()) {
            return false;
        }
    }

    if let Some((integer, fraction)) = mantissa.split_once('.') {
        (integer.is_empty() || valid_digit_run(integer, |byte| byte.is_ascii_digit()))
            && (fraction.is_empty() || valid_digit_run(fraction, |byte| byte.is_ascii_digit()))
            && (!integer.is_empty() || !fraction.is_empty())
    } else {
        valid_digit_run(mantissa, |byte| byte.is_ascii_digit())
    }
}

fn sqlite_type_name_trivia(
    raw: &str,
    cursor: &mut usize,
) -> Result<Vec<SqliteTypeNameTrivia>, AnalysisError> {
    let bytes = raw.as_bytes();
    let mut trivia = Vec::new();

    loop {
        let start = *cursor;
        while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
            *cursor += 1;
        }
        if *cursor > start {
            trivia.push(SqliteTypeNameTrivia::Whitespace(
                raw[start..*cursor].to_owned(),
            ));
        }
        if bytes.get(*cursor..*cursor + 2) == Some(b"/*") {
            let start = *cursor;
            *cursor += 2;
            while *cursor + 1 < bytes.len() && bytes.get(*cursor..*cursor + 2) != Some(b"*/") {
                *cursor += 1;
            }
            if *cursor + 1 >= bytes.len() {
                return AnalysisError::raise_unexpected_input("SQLite type name comment");
            }
            *cursor += 2;
            trivia.push(SqliteTypeNameTrivia::BlockComment(
                raw[start..*cursor].to_owned(),
            ));
            continue;
        }
        if bytes.get(*cursor..*cursor + 2) == Some(b"--") {
            let start = *cursor;
            *cursor += 2;
            while *cursor < bytes.len() && !matches!(bytes[*cursor], b'\r' | b'\n') {
                *cursor += 1;
            }
            trivia.push(SqliteTypeNameTrivia::LineComment(
                raw[start..*cursor].to_owned(),
            ));
            continue;
        }
        break;
    }

    Ok(trivia)
}

impl Call {
    fn base(name: &str) -> Self {
        Self {
            sql_syntax: false,
            kind: CallKind::ordinary(),
            name: Name::local(name),
            arguments: Vec::new(),
            argument_locations: Vec::new(),
            star: false,
            distinct: false,
            variadic: false,
            within_group: false,
            within_order: Vec::new(),
            order: Vec::new(),
            filter: None,
            over: None,
            location: None,
        }
    }
}

impl TryFrom<&syn::ExprFunctionCall> for Call {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprFunctionCall) -> Result<Self, Self::Error> {
        let mut call = Self::base(value.name().value());
        call.arguments = Expr::try_from_list(value.args())?;
        call.distinct = matches!(value.distinctness(), Some(syn::Distinctness::Distinct));
        call.order = Order::try_from_list(value.order_by())?;
        if !value.syntax_within_group().is_empty() {
            call.within_group = true;
            call.within_order = Order::try_from_list(value.syntax_within_group())?;
        }
        call.apply_tail(value.filter_over())?;
        Ok(call)
    }
}

impl Call {
    fn apply_tail(&mut self, tail: &syn::FunctionTail) -> Result<(), AnalysisError> {
        self.filter = tail
            .filter_clause()
            .as_deref()
            .map(Expr::try_from)
            .transpose()?;
        self.over = tail
            .over_clause()
            .as_ref()
            .map(Window::try_from)
            .transpose()?;
        Ok(())
    }
}

impl TryFrom<&syn::Expr> for Access {
    type Error = AnalysisError;

    fn try_from(value: &syn::Expr) -> Result<Self, Self::Error> {
        Ok(Self::Index(Expr::try_from(value)?))
    }
}

impl From<&syn::Name> for Access {
    fn from(value: &syn::Name) -> Self {
        Self::Field(value.value().to_owned())
    }
}

const fn operator_name(value: &syn::Operator) -> &'static str {
    match value {
        syn::Operator::Add => "+",
        syn::Operator::Subtract => "-",
        syn::Operator::Multiply => "*",
        syn::Operator::Divide => "/",
        syn::Operator::Modulus => "%",
        syn::Operator::Equals => "=",
        syn::Operator::NotEquals => "<>",
        syn::Operator::Grater => ">",
        syn::Operator::GraterEquals => ">=",
        syn::Operator::Less => "<",
        syn::Operator::LessEqales => "<=",
        syn::Operator::Is => "is",
        syn::Operator::IsNot => "is not",
        syn::Operator::Concat => "||",
        syn::Operator::BitwiseAnd => "&",
        syn::Operator::BitwiseOr => "|",
        syn::Operator::BitwiseNot => "~",
        syn::Operator::LeftShift => "<<",
        syn::Operator::RightShift => ">>",
        syn::Operator::ArrowRight => "->",
        syn::Operator::ArrowRightShift => "->>",
        syn::Operator::ArrayContains => "@>",
        syn::Operator::ArrayOverlap => "&&",
        syn::Operator::And => "and",
        syn::Operator::Or => "or",
    }
}

impl TryFrom<&syn::Parenthesized> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::Parenthesized) -> Result<Self, Self::Error> {
        Ok(Self::Parenthesized(Self::try_from_list(value.expr())?))
    }
}

impl TryFrom<&syn::ExprUnary> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprUnary) -> Result<Self, Self::Error> {
        Ok({
            let argument = Self::try_from(value.expr())?;
            match value.unary_operator() {
                syn::UnaryOperator::Not => Self::boolean(BooleanExpr::not(argument)),
                syn::UnaryOperator::Positive => Self::ordinary_operator("+", None, Some(argument)),
                syn::UnaryOperator::Negative => Self::ordinary_operator("-", None, Some(argument)),
                syn::UnaryOperator::BitwiseNot => {
                    Self::ordinary_operator("~", None, Some(argument))
                }
            }
        })
    }
}

impl TryFrom<&syn::ExprBinary> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprBinary) -> Result<Self, Self::Error> {
        Ok({
            let left = Self::try_from(value.left())?;
            let right = Self::try_from(value.right())?;
            match value.operator() {
                syn::Operator::And => Self::boolean(BooleanExpr::and(left, right)),
                syn::Operator::Or => Self::boolean(BooleanExpr::or(left, right)),
                operator => {
                    Self::ordinary_operator(operator_name(operator), Some(left), Some(right))
                }
            }
        })
    }
}

impl TryFrom<&syn::ExprBetween> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprBetween) -> Result<Self, Self::Error> {
        Ok(Self::operator(OperatorExpr::try_from(value)?))
    }
}

impl TryFrom<&syn::ExprBetween> for OperatorExpr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprBetween) -> Result<Self, Self::Error> {
        Ok(Self {
            name: Name::default(),
            kind: if *value.not() {
                OperatorKind::not_between()
            } else {
                OperatorKind::between()
            },
            left: Some(Expr::try_from(value.lhs())?),
            right: Some(Expr::List(vec![
                Expr::try_from(value.start())?,
                Expr::try_from(value.end())?,
            ])),
            escape: None,
        })
    }
}

impl OperatorExpr {
    fn ordinary(name: &str, left: Option<Expr>, right: Option<Expr>) -> Self {
        Self {
            name: Name::local(name),
            kind: OperatorKind::ordinary(),
            left,
            right,
            escape: None,
        }
    }
}

impl BooleanExpr {
    fn not(argument: Expr) -> Self {
        Self {
            operator: BooleanOperator::not(),
            arguments: vec![argument],
        }
    }

    fn and(left: Expr, right: Expr) -> Self {
        Self::binary(BooleanOperator::and(), left, right)
    }

    fn or(left: Expr, right: Expr) -> Self {
        Self::binary(BooleanOperator::or(), left, right)
    }

    fn binary(operator: BooleanOperator, left: Expr, right: Expr) -> Self {
        Self {
            operator,
            arguments: vec![left, right],
        }
    }
}

impl TryFrom<&syn::ExprIsNull> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprIsNull) -> Result<Self, Self::Error> {
        Ok(Self::null_test(NullTest::try_from(value)?))
    }
}

impl TryFrom<&syn::ExprNotNull> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprNotNull) -> Result<Self, Self::Error> {
        Ok(Self::null_test(NullTest::try_from(value)?))
    }
}

impl TryFrom<&syn::ExprCollate> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprCollate) -> Result<Self, Self::Error> {
        Ok(Self::collate(Collate::try_from(value)?))
    }
}

impl TryFrom<&syn::ExprCast> for Expr {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprCast) -> Result<Self, Self::Error> {
        Ok(Self::cast(Cast::try_from(value)?))
    }
}

impl TryFrom<&syn::ExprIsNull> for NullTest {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprIsNull) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from(value.expr())?,
            negated: false,
            row: false,
        })
    }
}

impl TryFrom<&syn::ExprNotNull> for NullTest {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprNotNull) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from(value.expr())?,
            negated: true,
            row: false,
        })
    }
}

impl TryFrom<&syn::ExprCollate> for Collate {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprCollate) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from(value.expr())?,
            name: Name::local(value.name().value()),
            location: None,
        })
    }
}

impl TryFrom<&syn::ExprCast> for Cast {
    type Error = AnalysisError;

    fn try_from(value: &syn::ExprCast) -> Result<Self, Self::Error> {
        Ok(Self {
            argument: Expr::try_from(value.expr())?,
            target: match value.type_name() {
                Some(target) => TypeName::try_from(target)?,
                None => TypeName::empty(),
            },
        })
    }
}

impl Expr {
    fn null_test(test: NullTest) -> Self {
        Self::NullTest(Box::new(test))
    }

    fn collate(collate: Collate) -> Self {
        Self::Collate(Box::new(collate))
    }

    fn cast(cast: Cast) -> Self {
        Self::Cast(Box::new(cast))
    }
}
