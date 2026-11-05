#[cfg(feature = "sqlite")]
use crate::syn::sqlite::CheckColumnConstraint as SqliteCheckColumnConstraint;
use crate::{
    ast::{TableConstraint, TryFromWithTokens},
    sem::{CheckConstraint, ColumnRename, traversal::SemanticVisitorMut},
    syn::Constraint,
};
use sql_fun_core::IVec;

#[cfg(feature = "sqlite")]
use crate::{
    SqliteToken,
    ast::{ColumnConstraint as AstColumnConstraint, Expr},
};
use crate::{
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ParseContext,
        SemScalarExpr,
    },
    syn::ScanToken,
};

/// column value constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct CheckColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    expr: SemScalarExpr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    analysis: Option<Box<InlineCheckAnalysis>>,
}

#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
struct InlineCheckAnalysis {
    name: String,
    applicable: bool,
    no_inherit: bool,
    enforced: Option<bool>,
}

impl CheckColumnConstraint {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
    }

    pub(super) fn rename_expression(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        self.expr.walk(visitor)
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
    }

    /// Construct an unanalyzed CHECK; `can_apply()` remains false until analysis.
    pub fn new(table_name: &TableName, column_name: &ColumnName, expr: SemScalarExpr) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            expr,
            analysis: None,
        }
    }

    pub(crate) fn from_analyzed(
        table: &TableName,
        column: &ColumnName,
        check: &CheckConstraint,
    ) -> Self {
        let mut result = Self::new(table, column, check.expression().clone());
        result.analysis = Some(Box::new(InlineCheckAnalysis {
            name: check.name().to_owned(),
            applicable: check.can_apply(),
            no_inherit: check.no_inherit(),
            enforced: check.enforced(),
        }));
        result
    }

    /// Explicit constraint name, when supplied in analyzed SQL.
    pub fn name(&self) -> Option<&str> {
        self.analysis
            .as_ref()
            .map(|analysis| analysis.name.as_str())
            .filter(|name| !name.is_empty())
    }

    /// Whether completed CHECK analysis permits catalog application.
    /// Unanalyzed constructor values and legacy serialized values are not applicable.
    pub fn can_apply(&self) -> bool {
        self.analysis
            .as_ref()
            .is_some_and(|analysis| analysis.applicable)
    }

    /// Whether NO INHERIT was declared.
    pub fn no_inherit(&self) -> bool {
        self.analysis
            .as_ref()
            .is_some_and(|analysis| analysis.no_inherit)
    }

    /// Explicit enforcement choice; omission remains absent.
    pub fn enforced(&self) -> Option<bool> {
        if let Some(analysis) = &self.analysis {
            analysis.enforced
        } else {
            None
        }
    }

    /// get constraint expression
    pub const fn expr(&self) -> &SemScalarExpr {
        &self.expr
    }
}

impl ColumnConstraintTrait for CheckColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Check
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl CheckColumnConstraint {
    /// analyze [`crate::syn::Constraint`]
    pub fn analyze_constraint<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &Constraint,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        Self::from_ast_legacy_inline(
            &TableConstraint::try_from_with_tokens(syn, tokens)?,
            context,
            table_name,
            column_name,
        )
    }

    /// analyze [`crate::syn::sqlite::CheckColumnConstraint`]
    #[cfg(feature = "sqlite")]
    pub fn analyze_constraint_sqlite<TParseContext>(
        context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &SqliteCheckColumnConstraint,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(ColumnConstraint, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let syntax = AstColumnConstraint::check(&Expr::try_from(syn.expr())?);
        let (check, context) = Self::from_ast_legacy_inline(
            &TableConstraint::unnamed_check(&syntax),
            context,
            table_name,
            column_name,
        )?;
        Ok((ColumnConstraint::check(check), context))
    }
}

impl ColumnConstraint {
    /// create Check variant
    pub const fn check(value: CheckColumnConstraint) -> Self {
        Self::Check(value)
    }

    /// get check variant
    pub const fn as_check(&self) -> Option<&CheckColumnConstraint> {
        if let Self::Check(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
