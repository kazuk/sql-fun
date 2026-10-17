//! Schema producers for writable-view metadata. No body is executed here.
use crate::ast::{Expr, FromItem, Name, Query, QueryBody, View};
use crate::sem::{
    AnalysisError, AstAndContextPair, FullName, ParseContext, SelectStatement, SemAst,
    ViewWriteChange, ViewWriteInfo,
    view_write::{WriteAction, WriteEvent, WriteObject},
};
use crate::syn::{ListOpt, Opt};

impl View {
    pub(super) fn write_name(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<Option<FullName>, AnalysisError> {
        qualify_declaration(&self.name.full_name()?, context)
    }
    pub(super) fn write_info(
        &self,
        context: &mut impl ParseContext,
        query: &SelectStatement,
        name: &FullName,
    ) -> Result<Option<ViewWriteInfo>, AnalysisError> {
        let mut source = self.query.automatic_view_source(context)?;
        if source.is_some() && !crate::sem::view_write::projection_preserves_rows(context, query)? {
            source = None;
        }
        let mut projections = query.result_columns().view_projection();
        if let Some(source) = &source {
            self.query
                .refine_view_cast_lineage(context, source, &mut projections)?;
        }
        let mut replacement = ViewWriteInfo::new(
            name,
            source.as_ref(),
            &projections,
            &query.result_columns().view_expressions(),
        );
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

fn qualify_declaration(
    name: &FullName,
    context: &mut impl ParseContext,
) -> Result<Option<FullName>, AnalysisError> {
    if name.schema().is_some() {
        return Ok(Some(name.clone()));
    }
    let Some(schema) = context.get_default_schema() else {
        context.report_analysis_problem(crate::sem::AnalysisProblem::view(
            &crate::sem::problem::ViewProblem::invalid_target(name),
        ))?;
        return Ok(None);
    };
    Ok(Some(name.clone().set_schema(schema)))
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
    fn refine_view_cast_lineage(
        &self,
        context: &impl ParseContext,
        source: &FullName,
        projections: &mut [(crate::sem::ColumnDefinition, Option<crate::sem::ColumnName>)],
    ) -> Result<(), AnalysisError> {
        let QueryBody::Select(select) = &self.body else {
            return Ok(());
        };
        let columns = if let Some(table) = context.get_table_impl(source) {
            let mut columns = Vec::new();
            while let Some(column) = table.get_column_at(columns.len()) {
                columns.push(column.clone());
            }
            columns
        } else if let Some(view) = context.get_view_impl(source) {
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
        Ok(crate::sem::search_path::expand_search_path_inst(
            context,
            &requested,
            |key| {
                if context.get_table_impl(key).is_some() || context.get_view_impl(key).is_some() {
                    Some(key.clone())
                } else {
                    None
                }
            },
        ))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WriteDeclaration {
    relation: Name,
    action: WriteAction,
    trigger_lookup: bool,
    missing_ok: bool,
}
impl WriteDeclaration {
    fn new(relation: &Name, action: &WriteAction) -> Self {
        Self {
            relation: relation.clone(),
            action: action.clone(),
            trigger_lookup: false,
            missing_ok: false,
        }
    }
    pub(super) fn resolve_and_apply<C: ParseContext>(
        &self,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let requested = self.relation.full_name()?;
        let bound = if self.trigger_lookup {
            let Some(trigger) = qualify_declaration(&requested, &mut context)? else {
                return Ok(AstAndContextPair::new(
                    SemAst::unresolved_statement(),
                    context,
                ));
            };
            context.view_trigger_target(&trigger)
        } else {
            crate::sem::search_path::expand_search_path_inst(&context, &requested, |key| {
                context
                    .get_view_impl(key)
                    .filter(|view| !view.is_materialized())
                    .map(|_| key.clone())
            })
        };
        let Some(view) = bound else {
            if !self.trigger_lookup
                && context
                    .get_table(&crate::sem::TableName::from(requested.clone()))
                    .is_some()
            {
                context.report_analysis_problem(crate::sem::AnalysisProblem::view(
                    &crate::sem::problem::ViewProblem::invalid_target(&requested),
                ))?;
                return Ok(AstAndContextPair::new(
                    SemAst::unresolved_statement(),
                    context,
                ));
            }
            if self.missing_ok {
                return Ok(AstAndContextPair::new(
                    SemAst::unresolved_statement(),
                    context,
                ));
            }
            context.report_analysis_problem(crate::sem::AnalysisProblem::view(
                &crate::sem::problem::ViewProblem::invalid_target(&requested),
            ))?;
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        };
        if crate::ast::Dialect::from(context.metadata()) == crate::ast::Dialect::sqlite()
            && let Some((trigger, if_not_exists)) = self.action.trigger_declaration()
            && let Some(_owner) = context.view_trigger_target(&FullName::new(
                &None,
                &view.schema().cloned(),
                &crate::sem::LocalName::from(trigger),
            ))
        {
            if !if_not_exists {
                context.report_analysis_problem(crate::sem::AnalysisProblem::view(
                    &crate::sem::problem::ViewProblem::duplicate_object(&view, trigger),
                ))?;
            }
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let change = ViewWriteChange::new(&view, &self.action);
        let context = context.apply_view_write_change(&change)?;
        Ok(AstAndContextPair::new(
            SemAst::view_write_change(&change),
            context,
        ))
    }
}

impl TryFrom<&crate::syn::CreateTrigStmt> for WriteDeclaration {
    type Error = AnalysisError;
    fn try_from(value: &crate::syn::CreateTrigStmt) -> Result<Self, Self::Error> {
        let relation =
            super::postgres::required(value.get_relation().as_inner(), "trigger target")?;
        let mut columns = Vec::new();
        for node in value.get_columns().as_inner().unwrap_or_default() {
            columns.push(super::postgres::required(
                node.as_string().get_sval(),
                "trigger column",
            )?);
        }
        let object = WriteObject::trigger(
            &value.get_trigname(),
            value.get_events(),
            value.get_timing() & 64 != 0,
            &columns,
        );
        Ok(Self::new(
            &Name::from(&relation),
            &WriteAction::object(&object, value.get_replace(), false),
        ))
    }
}
impl TryFrom<&crate::syn::RuleStmt> for WriteDeclaration {
    type Error = AnalysisError;
    fn try_from(value: &crate::syn::RuleStmt) -> Result<Self, Self::Error> {
        let relation = super::postgres::required(value.get_relation().as_inner(), "rule target")?;
        let event = WriteEvent::try_from(value.get_event().as_inner())?;
        let actions = value.get_actions().as_inner().unwrap_or_default();
        let returning = actions.iter().any(rule_action_has_returning);
        let object = WriteObject::rule(
            &value.get_rulename(),
            event,
            value.get_instead(),
            value.get_where_clause().as_inner().is_some(),
            returning,
            &actions,
        );
        Ok(Self::new(
            &Name::from(&relation),
            &WriteAction::object(&object, value.get_replace(), false),
        ))
    }
}
fn rule_action_has_returning(node: &crate::syn::Node) -> bool {
    node.as_insert_stmt()
        .get_returning_list()
        .as_inner()
        .is_some_and(|list| !list.is_empty())
        || node
            .as_update_stmt()
            .get_returning_list()
            .as_inner()
            .is_some_and(|list| !list.is_empty())
        || node
            .as_delete_stmt()
            .get_returning_list()
            .as_inner()
            .is_some_and(|list| !list.is_empty())
}
impl TryFrom<Option<crate::syn::CmdType>> for WriteEvent {
    type Error = AnalysisError;
    fn try_from(value: Option<crate::syn::CmdType>) -> Result<Self, Self::Error> {
        match value {
            Some(crate::syn::CmdType::CmdInsert) => Ok(Self::insert()),
            Some(crate::syn::CmdType::CmdUpdate) => Ok(Self::update()),
            Some(crate::syn::CmdType::CmdDelete) => Ok(Self::delete()),
            _ => AnalysisError::raise_unexpected_input("view write rule event"),
        }
    }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&crate::syn::sqlite::CreateTriggerStatement> for WriteDeclaration {
    type Error = AnalysisError;
    fn try_from(value: &crate::syn::sqlite::CreateTriggerStatement) -> Result<Self, Self::Error> {
        let (events, columns) = sqlite_trigger_event(value.event());
        let object = WriteObject::trigger(
            value.trigger_name().name().value(),
            events,
            matches!(
                value.time(),
                Some(crate::syn::sqlite::TriggerTime::InsteadOf)
            ),
            &columns,
        );
        Ok(Self::new(
            &Name::from(value.table_name()),
            &WriteAction::object(&object, false, *value.if_not_exists()),
        ))
    }
}
#[cfg(feature = "sqlite")]
fn sqlite_trigger_event(event: &crate::syn::sqlite::TriggerEvent) -> (i32, Vec<String>) {
    use crate::syn::sqlite::TriggerEvent;
    match event {
        TriggerEvent::Insert => (4, Vec::new()),
        TriggerEvent::Update => (16, Vec::new()),
        TriggerEvent::UpdateOf(columns) => (
            16,
            columns.iter().map(|name| name.value().to_owned()).collect(),
        ),
        TriggerEvent::Delete => (8, Vec::new()),
    }
}

#[cfg(feature = "sqlite")]
impl TryFrom<&crate::syn::sqlite::DropTriggerStatement> for WriteDeclaration {
    type Error = AnalysisError;
    fn try_from(value: &crate::syn::sqlite::DropTriggerStatement) -> Result<Self, Self::Error> {
        let object = WriteObject::trigger(value.trigger_name().name().value(), 0, false, &[]);
        let mut declaration = Self::new(
            &Name::from(value.trigger_name()),
            &WriteAction::drop_object(&object, *value.if_exists()),
        );
        declaration.trigger_lookup = true;
        declaration.missing_ok = *value.if_exists();
        Ok(declaration)
    }
}
impl TryFrom<&crate::syn::DropStmt> for WriteDeclaration {
    type Error = AnalysisError;
    fn try_from(value: &crate::syn::DropStmt) -> Result<Self, Self::Error> {
        let objects = value.get_objects().as_inner().unwrap_or_default();
        if objects.len() != 1 {
            return AnalysisError::raise_unexpected_input(
                "one view trigger/rule drop target required",
            );
        }
        let mut parts = Name::try_from(objects[0].as_list().get_items())?.0;
        let Some(name) = parts.pop() else {
            return AnalysisError::raise_unexpected_input("trigger/rule drop name");
        };
        let object = if matches!(
            value.get_remove_type().as_inner(),
            Some(crate::syn::ObjectType::ObjectRule)
        ) {
            WriteObject::rule(&name, WriteEvent::insert(), false, false, false, &[])
        } else {
            WriteObject::trigger(&name, 0, false, &[])
        };
        let mut declaration = Self::new(
            &Name::from_parts(&parts),
            &WriteAction::drop_object(&object, value.get_missing_ok()),
        );
        declaration.missing_ok = value.get_missing_ok();
        Ok(declaration)
    }
}
