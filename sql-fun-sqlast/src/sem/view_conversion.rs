//! Preserve syntax-dependent view lineage while producing semantic definitions.

use crate::ast::{Cast, Name, Query, View, ViewCheckOption as AstViewCheckOption};
use crate::sem::{AstConversion, convert};

use crate::{
    ast::{Expr, FromItem, QueryBody, SyntaxValue},
    context::ParseContext,
    names::{ColumnName, FullName, TableName, ViewName},
    sem::{
        AnalysisError, ColumnDefinition, FromClause, ScalarAnalysisContext, ScalarConstExpr,
        SelectStatement, SemScalarExpr, TypeReference, ViewCheckOption, ViewColumnLineage,
        ViewOption, WithClause,
    },
};

impl AstConversion<'_, Expr> {
    // PostgreSQL removes an explicit cast to the same type without typmods.
    // Consult owned syntax here: the shared semantic type intentionally does
    // not retain typmods, so type equality alone cannot prove a cast is inert.
    pub(crate) fn view_cast_column<'a>(
        &self,
        context: &impl ParseContext,
        columns: &'a [ColumnDefinition],
        aliases: &[String],
    ) -> Result<Option<&'a ColumnDefinition>, AnalysisError> {
        match self.ast {
            Expr::Column(name) => Ok(convert(name).view_source_column(context, columns, aliases)),
            Expr::Cast(cast) => convert::<Cast>(cast).view_cast_column(context, columns, aliases),
            Expr::Parenthesized(values) if values.len() == 1 => {
                convert(&values[0]).view_cast_column(context, columns, aliases)
            }
            _ => Ok(None),
        }
    }
}

impl AstConversion<'_, Name> {
    fn view_source_column<'a>(
        &self,
        _context: &impl ParseContext,
        columns: &'a [ColumnDefinition],
        aliases: &[String],
    ) -> Option<&'a ColumnDefinition> {
        columns.iter().enumerate().find_map(|(index, column)| {
            let visible = match aliases.get(index) {
                Some(alias) => Some(alias.as_str()),
                None => column.name().as_ref().map(ColumnName::as_str),
            };
            if self
                .ast
                .0
                .last()
                .is_some_and(|name| visible == Some(name.as_str()))
            {
                Some(column)
            } else {
                None
            }
        })
    }
}

impl AstConversion<'_, Cast> {
    fn view_cast_column<'a>(
        &self,
        context: &impl ParseContext,
        columns: &'a [ColumnDefinition],
        aliases: &[String],
    ) -> Result<Option<&'a ColumnDefinition>, AnalysisError> {
        if !self.ast.target.modifiers.is_empty() {
            return Ok(None);
        }
        let Some(column) =
            convert(&self.ast.argument).view_cast_column(context, columns, aliases)?
        else {
            return Ok(None);
        };
        let target = TypeReference::concrete_type_ref(
            &convert(&self.ast.target.name).full_name()?,
            !self.ast.target.array_bounds.is_empty(),
        );
        if let Some(source) = column.get_type()
            && let Some(source) = context.canonicalize_type_ref(source)
            && context.canonicalize_type_ref(&target).as_ref() == Some(&source)
        {
            Ok(Some(column))
        } else {
            Ok(None)
        }
    }
}

impl AstConversion<'_, Query> {
    pub(crate) fn view_lineage(
        &self,
        context: &impl ParseContext,
        query: &SelectStatement,
    ) -> Result<ViewColumnLineage, AnalysisError> {
        let source = self.automatic_view_source(context)?;
        let mut projections = query.result_columns().view_projection();
        if let Some(source) = &source {
            self.refine_view_cast_lineage(context, source, &mut projections)?;
        }
        let sources: Vec<_> = projections.into_iter().map(|(_, source)| source).collect();
        Ok(ViewColumnLineage::new(&sources))
    }
    pub(crate) fn refine_view_cast_lineage(
        &self,
        context: &impl ParseContext,
        source: &FullName,
        projections: &mut [(ColumnDefinition, Option<ColumnName>)],
    ) -> Result<(), AnalysisError> {
        let QueryBody::Select(select) = &self.ast.body else {
            return Ok(());
        };
        let columns = if let Some(table) = context.get_table(&TableName::from(source.clone())) {
            let mut columns = Vec::new();
            while let Some(column) = table.get_column_at(columns.len()) {
                columns.push(column.clone());
            }
            columns
        } else if let Some(view) = context.get_view(&ViewName::from_full_name(source)) {
            if view.is_materialized() {
                return Ok(());
            }
            view.result_columns().column_definitions()?
        } else {
            return Ok(());
        };
        let aliases = match select.from.first() {
            Some(FromItem::Relation(relation)) => relation
                .alias
                .as_ref()
                .map(|alias| alias.columns.as_slice())
                .unwrap_or_default(),
            _ => &[],
        };
        let mut index = 0;
        for target in &select.targets {
            if matches!(&target.expression, Expr::Star(_)) {
                index += columns.len();
                continue;
            }
            if let Some((_, lineage)) = projections.get_mut(index)
                && lineage.is_none()
            {
                *lineage = if let Some(column) =
                    convert(&target.expression).view_cast_column(context, &columns, aliases)?
                {
                    column.name().clone()
                } else {
                    None
                };
            }
            index += 1;
        }
        Ok(())
    }

    pub(crate) fn automatic_view_source(
        &self,
        context: &impl ParseContext,
    ) -> Result<Option<FullName>, AnalysisError> {
        if !self.ast.with.entries.is_empty()
            || self.ast.limit.is_some()
            || self.ast.offset.is_some()
        {
            return Ok(None);
        }
        convert(&self.ast.body).automatic_view_source(context)
    }
}

impl AstConversion<'_, QueryBody> {
    pub(crate) fn automatic_view_source(
        &self,
        context: &impl ParseContext,
    ) -> Result<Option<FullName>, AnalysisError> {
        let QueryBody::Select(select) = self.ast else {
            return Ok(None);
        };
        if select.distinct.is_some()
            || !select.group.is_empty()
            || select.having.is_some()
            || !select.windows.is_empty()
            || select.from.len() != 1
        {
            return Ok(None);
        }
        convert(&select.from[0]).automatic_view_source(context)
    }
}

impl AstConversion<'_, FromItem> {
    pub(crate) fn automatic_view_source(
        &self,
        context: &impl ParseContext,
    ) -> Result<Option<FullName>, AnalysisError> {
        let FromItem::Relation(relation) = self.ast else {
            return Ok(None);
        };
        let requested = convert(&relation.name).full_name()?;
        Ok(context.resolve_relation_name(&requested))
    }
}

impl AstConversion<'_, View> {
    pub(crate) fn semantic_options(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Vec<ViewOption>, AnalysisError> {
        let mut options = Vec::new();
        for option in &self.ast.options {
            let value = match &option.value {
                None => None,
                Some(SyntaxValue::Expression(Expr::Column(name))) => Some(SemScalarExpr::literal(
                    &ScalarConstExpr::string(&name.0.join(".")),
                )),
                Some(SyntaxValue::Expression(expression)) => Some(convert(expression).resolve(
                    context,
                    &ScalarAnalysisContext::new(&WithClause::default(), &FromClause::default()),
                )?),
                Some(SyntaxValue::Type(_)) => {
                    return AnalysisError::raise_unexpected_input("View option type");
                }
            };
            options.push(ViewOption::new(
                option.namespace.as_deref(),
                &option.name,
                value.as_ref(),
            ));
        }
        Ok(options)
    }
}

impl AstConversion<'_, AstViewCheckOption> {
    pub(crate) const fn semantic(self) -> ViewCheckOption {
        match self.ast {
            AstViewCheckOption::None => ViewCheckOption::none(),
            AstViewCheckOption::Local => ViewCheckOption::local(),
            AstViewCheckOption::Cascaded => ViewCheckOption::cascaded(),
        }
    }
}
