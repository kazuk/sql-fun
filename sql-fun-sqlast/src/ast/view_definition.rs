//! Resolved View projection lineage retained independently of DML capabilities.
use crate::{
    ast::{Expr, FromItem, Query, QueryBody, View},
    context::ParseContext,
    sem::{
        AnalysisError, ColumnDefinition, ColumnName, FromClause, FullName, ScalarAnalysisContext,
        ScalarConstExpr, SelectStatement, SemScalarExpr, TableName, TypeReference, ViewCheckOption,
        ViewColumnLineage, ViewName, ViewOption, WithClause,
    },
};

impl Expr {
    // PostgreSQL removes an explicit cast to the same type without typmods.
    // Consult owned syntax here: the shared semantic type intentionally does
    // not retain typmods, so type equality alone cannot prove a cast is inert.
    fn view_cast_column<'a>(
        &self,
        context: &impl ParseContext,
        columns: &'a [ColumnDefinition],
        aliases: &[String],
    ) -> Result<Option<&'a ColumnDefinition>, AnalysisError> {
        match self {
            Self::Column(name) => Ok(columns.iter().enumerate().find_map(|(index, column)| {
                let visible = match aliases.get(index) {
                    Some(alias) => Some(alias.as_str()),
                    None => column.name().as_ref().map(ColumnName::as_str),
                };
                if name
                    .0
                    .last()
                    .is_some_and(|name| visible == Some(name.as_str()))
                {
                    Some(column)
                } else {
                    None
                }
            })),
            Self::Cast(cast) if cast.target.modifiers.is_empty() => {
                let Some(column) = cast.argument.view_cast_column(context, columns, aliases)?
                else {
                    return Ok(None);
                };
                let target = TypeReference::concrete_type_ref(
                    &cast.target.name.full_name()?,
                    !cast.target.array_bounds.is_empty(),
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
            Self::Parenthesized(values) if values.len() == 1 => {
                values[0].view_cast_column(context, columns, aliases)
            }
            _ => Ok(None),
        }
    }
}

impl Query {
    pub(in crate::ast) fn view_lineage(
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
    fn refine_view_cast_lineage(
        &self,
        context: &impl ParseContext,
        source: &FullName,
        projections: &mut [(ColumnDefinition, Option<ColumnName>)],
    ) -> Result<(), AnalysisError> {
        let QueryBody::Select(select) = &self.body else {
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
                *lineage = if let Some(column) = target
                    .expression
                    .view_cast_column(context, &columns, aliases)?
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

    fn automatic_view_source(
        &self,
        context: &impl ParseContext,
    ) -> Result<Option<FullName>, AnalysisError> {
        if !self.with.entries.is_empty() || self.limit.is_some() || self.offset.is_some() {
            return Ok(None);
        }
        self.body.automatic_view_source(context)
    }
}
impl QueryBody {
    fn automatic_view_source(
        &self,
        context: &impl ParseContext,
    ) -> Result<Option<FullName>, AnalysisError> {
        let Self::Select(select) = self else {
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
        select.from[0].automatic_view_source(context)
    }
}
impl FromItem {
    fn automatic_view_source(
        &self,
        context: &impl ParseContext,
    ) -> Result<Option<FullName>, AnalysisError> {
        let Self::Relation(relation) = self else {
            return Ok(None);
        };
        let requested = relation.name.full_name()?;
        Ok(context.resolve_relation_name(&requested))
    }
}

impl View {
    pub(super) fn semantic_options(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Vec<ViewOption>, AnalysisError> {
        let mut options = Vec::new();
        for option in &self.options {
            let value = match &option.value {
                None => None,
                Some(crate::ast::SyntaxValue::Expression(Expr::Column(name))) => Some(
                    SemScalarExpr::literal(&ScalarConstExpr::string(&name.0.join("."))),
                ),
                Some(crate::ast::SyntaxValue::Expression(expression)) => Some(expression.resolve(
                    context,
                    &ScalarAnalysisContext::new(&WithClause::default(), &FromClause::default()),
                )?),
                Some(crate::ast::SyntaxValue::Type(_)) => {
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
impl crate::ast::ViewCheckOption {
    pub(super) const fn semantic(self) -> ViewCheckOption {
        match self {
            Self::None => ViewCheckOption::none(),
            Self::Local => ViewCheckOption::local(),
            Self::Cascaded => ViewCheckOption::cascaded(),
        }
    }
}
