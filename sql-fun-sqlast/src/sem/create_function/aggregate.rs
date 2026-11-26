use super::{CreateFunction, FunctionDeclarationOutcome};
use crate::{
    ast::{
        create_aggregate::{
            AggregateArguments, AggregateOption, AggregateOptionValue, CreateAggregate,
        },
        ddl::ParameterMode,
    },
    names::{FullName, OperatorName},
    sem::{
        AnalysisError, FunctionOverloadCollection, OverloadVariant, ParallelSafety, ParseContext,
        TypeReference,
        type_system::{ExactFunction, FunctionParam},
    },
};

#[derive(Clone, Debug)]
pub(super) struct AggregateSupport {
    state: Option<TypeReference>,
    transition: Option<ExactFunction>,
    final_function: Option<ExactFunction>,
    combine: Option<ExactFunction>,
    sort_operator: Option<OperatorName>,
}

impl AggregateSupport {
    pub(super) fn visit_references(
        &self,
        visitor: &mut impl crate::sem::traversal::SemanticVisitor,
    ) -> Result<(), AnalysisError> {
        if let Some(state) = &self.state {
            visitor.schema_type(state)?;
        }
        for function in [&self.transition, &self.final_function, &self.combine]
            .into_iter()
            .flatten()
        {
            visitor.schema_function(function.name())?;
        }
        Ok(())
    }
}

impl CreateFunction {
    pub(crate) fn from_aggregate(
        ast: &CreateAggregate,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let AggregateArguments::Declared {
            parameters,
            direct_count: None,
        } = &ast.arguments
        else {
            todo!("unsupported CREATE AGGREGATE argument form");
        };
        if parameters.is_empty()
            || parameters
                .iter()
                .any(|parameter| !matches!(parameter.mode(), ParameterMode::In))
            || ast.options.iter().any(|option| {
                !matches!(
                    option.key.as_str(),
                    "stype"
                        | "sfunc"
                        | "finalfunc"
                        | "combinefunc"
                        | "sortop"
                        | "initcond"
                        | "parallel"
                )
            })
        {
            todo!("unsupported CREATE AGGREGATE form");
        }
        let name = FullName::from_ast_name(&ast.name)?;
        let mut outcome = FunctionDeclarationOutcome::default();
        let name = Self::shell_creation_name(&name, &name, &mut outcome, context)?.unwrap_or(name);
        let params = parameters
            .iter()
            .map(|param| FunctionParam::from_ast(param, context))
            .collect::<Result<Vec<_>, _>>()?;
        let inputs = params
            .iter()
            .map(|param| param.get_type().cloned())
            .collect::<Vec<_>>();
        let state = ast
            .options
            .iter()
            .find(|option| option.key == "stype")
            .and_then(|option| match &option.value {
                Some(AggregateOptionValue::Type(ty)) => Some(ty),
                _ => None,
            })
            .map(TypeReference::from_ast)
            .transpose()?
            .and_then(|ty| context.canonicalize_type_ref(&ty));
        let mut bindings = AggregateSupport {
            state: state.clone(),
            transition: None,
            final_function: None,
            combine: None,
            sort_operator: None,
        };
        let mut result = state.clone();
        if state.is_none() || inputs.iter().any(Option::is_none) {
            outcome.reject(
                &name,
                "aggregate input or state type is missing or unresolved".to_owned(),
                context,
            )?;
        }
        if !ast.options.iter().any(|option| option.key == "sfunc") {
            outcome.reject(
                &name,
                "aggregate transition function is required".to_owned(),
                context,
            )?;
        }
        let mut parallel_safety = None;
        let mut seen = std::collections::HashSet::new();
        for option in &ast.options {
            if !seen.insert(option.key.as_str()) {
                outcome.reject(
                    &name,
                    format!("duplicate aggregate option {}", option.key),
                    context,
                )?;
            }
            match option.key.as_str() {
                "sfunc" | "combinefunc" | "finalfunc" => {
                    let mut arguments = vec![state.clone()];
                    match option.key.as_str() {
                        "sfunc" => arguments.extend(inputs.iter().cloned()),
                        "combinefunc" => arguments.push(state.clone()),
                        _ => {}
                    }
                    if arguments.iter().all(Option::is_some) {
                        let resolved = support_function(option, &arguments, context)?;
                        let return_type = resolved
                            .as_ref()
                            .and_then(|function| function.overload().ret_type().clone());
                        if return_type.is_none()
                            || (option.key != "finalfunc" && return_type != state)
                        {
                            outcome.reject(
                                &name,
                                format!("invalid aggregate support function for {}", option.key),
                                context,
                            )?;
                        }
                        match option.key.as_str() {
                            "finalfunc" => {
                                result = return_type;
                                bindings.final_function = resolved;
                            }
                            "sfunc" => bindings.transition = resolved,
                            _ => bindings.combine = resolved,
                        }
                    }
                }
                "sortop" => {
                    let operator = option_name(option)
                        .as_ref()
                        .and_then(|name| OperatorName::from_ast_name(name).ok());
                    if let [Some(input)] = inputs.as_slice() {
                        bindings.sort_operator = operator.as_ref().and_then(|operator| {
                            crate::sem::search_path::expand_operator_search_path(
                                context,
                                operator,
                                |candidate| {
                                    context
                                        .get_operator_op(candidate, true, true)
                                        .filter(|definition| {
                                            definition.has_operands(Some(input), Some(input))
                                        })
                                        .map(|_| candidate.clone())
                                },
                            )
                        });
                        if bindings.sort_operator.is_none() {
                            outcome.reject(
                                &name,
                                "unresolved aggregate sort operator".to_owned(),
                                context,
                            )?;
                        }
                    } else {
                        outcome.reject(
                            &name,
                            "aggregate sort operator requires one input".to_owned(),
                            context,
                        )?;
                    }
                }
                "parallel" => {
                    parallel_safety =
                        option_name(option)
                            .as_ref()
                            .and_then(|name| match name.parts() {
                                [mode] if mode == "safe" => Some(ParallelSafety::Safe),
                                [mode] if mode == "restricted" => Some(ParallelSafety::Restricted),
                                [mode] if mode == "unsafe" => Some(ParallelSafety::Unsafe),
                                _ => None,
                            });
                    if parallel_safety.is_none() {
                        outcome.reject(
                            &name,
                            "invalid aggregate PARALLEL mode".to_owned(),
                            context,
                        )?;
                    }
                }
                _ => {}
            }
        }
        let overload = OverloadVariant::new(&result, &params, false, false);
        let mut definition = Self::new(&name, &FunctionOverloadCollection::new(&[overload]), &None)
            .with_replace(ast.replace);
        definition.set_builtin_properties(true, false);
        definition.parallel_safety = parallel_safety;
        definition.source = context.get_source_path_with_span().cloned();
        if matches!(outcome, FunctionDeclarationOutcome::Applicable(_))
            && let Some(conflict) = definition.declaration_conflict(context)
        {
            outcome = FunctionDeclarationOutcome::Rejected(
                "aggregate signature conflicts with an existing routine".to_owned(),
            );
            context.report_analysis_problem(conflict)?;
        }
        definition.aggregate_support = Some(bindings);
        definition.outcome = outcome;
        Ok(definition)
    }
}

fn option_name(option: &AggregateOption) -> Option<crate::ast::Name> {
    match &option.value {
        Some(AggregateOptionValue::Name(name)) => Some(name.clone()),
        Some(AggregateOptionValue::Literal(crate::ast::Literal::String(name))) => {
            Some(crate::ast::Name::local(name))
        }
        _ => None,
    }
}

fn support_function(
    option: &AggregateOption,
    arguments: &[Option<TypeReference>],
    context: &impl ParseContext,
) -> Result<Option<ExactFunction>, AnalysisError> {
    let name = option_name(option)
        .as_ref()
        .map(FullName::from_ast_name)
        .transpose()?;
    Ok(name
        .and_then(|name| context.find_exact_function(&name, arguments))
        .filter(|function| {
            !context.function_returns_set(function.name(), function.overload())
                && !context
                    .routine_definitions(function.name())
                    .iter()
                    .any(|definition| {
                        definition.catalog_aggregate()
                            && definition
                                .overloads()
                                .contains_instantiation(function.overload(), context)
                    })
        }))
}
