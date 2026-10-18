//! Schema producers for writable-view metadata. No body is executed here.
use crate::ast::{Expr, FromItem, Query, QueryBody, View};
use crate::sem::{AnalysisError, FullName, ParseContext, SelectStatement, ViewWriteInfo};

impl View {
    pub(super) fn write_info(
        &self,
        context: &mut impl ParseContext,
        query: &SelectStatement,
        name: &FullName,
    ) -> Result<Option<ViewWriteInfo>, AnalysisError> {
        let mut replacement = self.query.rule_write_info(context, query, name)?;
        replacement.retain_options(&self.options, self.check_option);
        if self.replace
            && let Some(existing) = context.view_write_info(name)
        {
            let mut retained = existing.clone();
            if let Err(problem) = retained.replace_definition(&replacement) {
                context.report_analysis_problem(crate::sem::AnalysisProblem::view(&problem))?;
                return Ok(None);
            }
            return Ok(Some(retained));
        }
        Ok(Some(replacement))
    }
}

impl Expr {
    // PostgreSQL removes an explicit cast to the same type without typmods.
    // Consult owned syntax here: the shared semantic type intentionally does
    // not retain typmods, so type equality alone cannot prove a cast is inert.
    fn view_cast_column<'a>(
        &self,
        context: &impl ParseContext,
        columns: &'a [crate::sem::ColumnDefinition],
        aliases: &[String],
    ) -> Result<Option<&'a crate::sem::ColumnDefinition>, AnalysisError> {
        match self {
            Self::Column(name) => Ok(columns.iter().enumerate().find_map(|(index, column)| {
                let visible = match aliases.get(index) {
                    Some(alias) => Some(alias.as_str()),
                    None => column.name().as_ref().map(crate::sem::ColumnName::as_str),
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
                let target = crate::sem::TypeReference::concrete_type_ref(
                    &cast.target.name.full_name()?,
                    !cast.target.array_bounds.is_empty(),
                );
                if context.canonicalize_type_ref(&target).as_ref() == column.get_type() {
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
    pub(in crate::ast) fn rule_write_info(
        &self,
        context: &impl ParseContext,
        query: &SelectStatement,
        name: &FullName,
    ) -> Result<ViewWriteInfo, AnalysisError> {
        let mut source = self.automatic_view_source(context)?;
        if source.is_some() && !crate::sem::view_write::projection_preserves_rows(context, query)? {
            source = None;
        }
        let mut projections = query.result_columns().view_projection();
        if let Some(source) = &source {
            self.refine_view_cast_lineage(context, source, &mut projections)?;
        }
        Ok(ViewWriteInfo::new(
            name,
            source.as_ref(),
            &projections,
            &query.result_columns().view_expressions(),
        ))
    }
    fn refine_view_cast_lineage(
        &self,
        context: &impl ParseContext,
        source: &FullName,
        projections: &mut [(crate::sem::ColumnDefinition, Option<crate::sem::ColumnName>)],
    ) -> Result<(), AnalysisError> {
        let QueryBody::Select(select) = &self.body else {
            return Ok(());
        };
        let columns = if let Some(table) =
            context.get_table(&crate::sem::TableName::from(source.clone()))
        {
            let mut columns = Vec::new();
            while let Some(column) = table.get_column_at(columns.len()) {
                columns.push(column.clone());
            }
            columns
        } else if let Some(view) = context.get_view(&crate::sem::ViewName::from_full_name(source)) {
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
