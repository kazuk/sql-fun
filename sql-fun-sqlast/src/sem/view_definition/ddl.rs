//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{
        Cast as AstCast, Expr as AstExpr, FromItem as AstFromItem, Name as AstName,
        Query as AstQuery, QueryBody as AstQueryBody, ViewCheckOption as AstViewCheckOption,
    },
    context::ParseContext,
    names::{ColumnName, FullName, TableName, ViewName},
    sem::{
        AnalysisError, ColumnDefinition, SelectStatement, TypeReference, ViewCheckOption,
        ViewColumnLineage,
    },
};

impl ViewColumnLineage {
    // PostgreSQL removes an explicit cast to the same type without typmods.
    // Consult owned syntax here: the shared semantic type intentionally does
    // not retain typmods, so type equality alone cannot prove a cast is inert.
    pub(crate) fn column_from_ast_expression<'a>(
        ast: &AstExpr,
        context: &impl ParseContext,
        columns: &'a [ColumnDefinition],
        aliases: &[String],
    ) -> Result<Option<&'a ColumnDefinition>, AnalysisError> {
        match ast {
            AstExpr::Column(name) => {
                Ok(Self::column_from_ast_name(name, context, columns, aliases))
            }
            AstExpr::Cast(cast) => Self::column_from_ast_cast(cast, context, columns, aliases),
            AstExpr::Parenthesized(values) if values.len() == 1 => {
                Self::column_from_ast_expression(&values[0], context, columns, aliases)
            }
            _ => Ok(None),
        }
    }

    fn column_from_ast_name<'a>(
        ast: &AstName,
        _context: &impl ParseContext,
        columns: &'a [ColumnDefinition],
        aliases: &[String],
    ) -> Option<&'a ColumnDefinition> {
        columns.iter().enumerate().find_map(|(index, column)| {
            let visible = match aliases.get(index) {
                Some(alias) => Some(alias.as_str()),
                None => column.name().as_ref().map(ColumnName::as_str),
            };
            if ast
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

    fn column_from_ast_cast<'a>(
        ast: &AstCast,
        context: &impl ParseContext,
        columns: &'a [ColumnDefinition],
        aliases: &[String],
    ) -> Result<Option<&'a ColumnDefinition>, AnalysisError> {
        if !ast.target.modifiers.is_empty() {
            return Ok(None);
        }
        let Some(column) =
            Self::column_from_ast_expression(&ast.argument, context, columns, aliases)?
        else {
            return Ok(None);
        };
        let target = TypeReference::concrete_type_ref(
            &FullName::from_ast_name(&ast.target.name)?,
            !ast.target.array_bounds.is_empty(),
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

    pub(crate) fn from_ast_query(
        ast: &AstQuery,
        context: &impl ParseContext,
        query: &SelectStatement,
    ) -> Result<Self, AnalysisError> {
        let source = Self::source_from_ast_query(ast, context)?;
        let mut projections = query.result_columns().view_projection();
        if let Some(source) = &source {
            Self::refine_ast_casts(ast, context, source, &mut projections)?;
        }
        let sources: Vec<_> = projections.into_iter().map(|(_, source)| source).collect();
        Ok(Self::new(&sources))
    }

    pub(crate) fn refine_ast_casts(
        ast: &AstQuery,
        context: &impl ParseContext,
        source: &FullName,
        projections: &mut [(ColumnDefinition, Option<ColumnName>)],
    ) -> Result<(), AnalysisError> {
        let AstQueryBody::Select(select) = &ast.body else {
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
            Some(AstFromItem::Relation(relation)) => relation
                .alias
                .as_ref()
                .map(|alias| alias.columns.as_slice())
                .unwrap_or_default(),
            _ => &[],
        };
        let mut index = 0;
        for target in &select.targets {
            if matches!(&target.expression, AstExpr::Star(_)) {
                index += columns.len();
                continue;
            }
            if let Some((_, lineage)) = projections.get_mut(index)
                && lineage.is_none()
            {
                *lineage = if let Some(column) = Self::column_from_ast_expression(
                    &target.expression,
                    context,
                    &columns,
                    aliases,
                )? {
                    column.name().clone()
                } else {
                    None
                };
            }
            index += 1;
        }
        Ok(())
    }

    pub(crate) fn source_from_ast_query(
        ast: &AstQuery,
        context: &impl ParseContext,
    ) -> Result<Option<FullName>, AnalysisError> {
        if !ast.with.entries.is_empty() || ast.limit.is_some() || ast.offset.is_some() {
            return Ok(None);
        }
        Self::source_from_ast_query_body(&ast.body, context)
    }

    pub(crate) fn source_from_ast_query_body(
        ast: &AstQueryBody,
        context: &impl ParseContext,
    ) -> Result<Option<FullName>, AnalysisError> {
        let AstQueryBody::Select(select) = ast else {
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
        Self::source_from_ast_from_item(&select.from[0], context)
    }

    pub(crate) fn source_from_ast_from_item(
        ast: &AstFromItem,
        context: &impl ParseContext,
    ) -> Result<Option<FullName>, AnalysisError> {
        let AstFromItem::Relation(relation) = ast else {
            return Ok(None);
        };
        let requested = FullName::from_ast_name(&relation.name)?;
        Ok(context.resolve_relation_name(&requested))
    }
}

impl ViewCheckOption {
    pub(crate) const fn from_ast(ast: &AstViewCheckOption) -> Self {
        match ast {
            AstViewCheckOption::None => Self::none(),
            AstViewCheckOption::Local => Self::local(),
            AstViewCheckOption::Cascaded => Self::cascaded(),
        }
    }
}
