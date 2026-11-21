use crate::{
    ast::{CreateType, Expr, Literal, SyntaxValue},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, AstAndContextPair, SemAst, TypeDefinition, TypeReference,
        pg_type::TypeCategory,
    },
};

pub(super) fn analyze(
    ast: &CreateType,
    context: &mut impl ParseContext,
) -> Result<SemAst, AnalysisError> {
    let name = context.apply_default_schema(&FullName::from_ast_name(ast.name())?)?;
    let reference = TypeReference::concrete_type_ref(&name, false);
    let Some(options) = ast.definition() else {
        return if context.permits_type_creation(&name)? {
            Ok(SemAst::CreateBaseType(TypeDefinition::Shell(reference)))
        } else {
            Ok(SemAst::unresolved_statement())
        };
    };
    match context.get_type_impl(&name) {
        Some(definition) if definition.is_shell() => {}
        Some(_) => {
            context.permits_type_creation(&name)?;
            return Ok(SemAst::unresolved_statement());
        }
        None => return Err(invalid("a shell declaration is required")),
    }

    let mut support = std::collections::BTreeMap::new();
    let mut category = TypeCategory::try_from('U').expect("valid default category");
    let mut collatable = false;
    for option in options {
        let option = option.syntax();
        let value = option.value();
        match option.name().as_str() {
            "input" | "output" | "receive" | "send" | "typmod_in" | "typmod_out" | "analyze"
            | "subscript" => {
                support.insert(option.name().as_str(), option_name(value.as_ref())?);
            }
            "like" | "element" => {
                let target = option_name(value.as_ref())?;
                if context.get_type(&target).is_none() {
                    return Err(invalid(&format!("type {target} does not exist")));
                }
            }
            "category" => {
                let Some(SyntaxValue::Expression(Expr::Literal(Literal::String(code)))) = value
                else {
                    return Err(invalid("CATEGORY requires a single character"));
                };
                let mut chars = code.chars();
                category = match (chars.next(), chars.next()) {
                    (Some(code), None) => TypeCategory::try_from(code).map_err(invalid)?,
                    _ => return Err(invalid("CATEGORY requires a single character")),
                };
            }
            "collatable" => {
                collatable = match value {
                    None => true,
                    Some(SyntaxValue::Expression(Expr::Literal(Literal::Boolean(value)))) => *value,
                    Some(SyntaxValue::Expression(Expr::Column(name)))
                        if name.parts() == ["true"] =>
                    {
                        true
                    }
                    Some(SyntaxValue::Expression(Expr::Column(name)))
                        if name.parts() == ["false"] =>
                    {
                        false
                    }
                    _ => return Err(invalid("COLLATABLE requires a boolean")),
                };
            }
            _ => {}
        }
    }
    for required in ["input", "output"] {
        if !support.contains_key(required) {
            return Err(invalid(&format!("{required} function is required")));
        }
    }
    for (role, routine) in support {
        check_support_function(context, &reference, role, &routine)?;
    }
    Ok(SemAst::CreateBaseType(TypeDefinition::base(
        &name, category, collatable,
    )))
}

pub(super) fn apply<T: ParseContext>(
    ast: &CreateType,
    mut context: T,
) -> Result<AstAndContextPair<T>, AnalysisError> {
    let statement = analyze(ast, &mut context)?;
    if let SemAst::CreateBaseType(definition) = &statement {
        let name = definition.full_name().expect("named definition");
        context.insert_shell_or_base_type(name, definition);
    }
    Ok(AstAndContextPair::new(statement, context))
}

fn option_name(value: Option<&SyntaxValue>) -> Result<FullName, AnalysisError> {
    match value {
        Some(SyntaxValue::Type(ty)) => FullName::from_ast_name(ty.name()),
        Some(SyntaxValue::Expression(Expr::Column(name))) => FullName::from_ast_name(name),
        _ => Err(invalid("expected a qualified name or identifier")),
    }
}

fn check_support_function(
    context: &impl ParseContext,
    base: &TypeReference,
    role: &str,
    routine: &FullName,
) -> Result<(), AnalysisError> {
    let builtin =
        |name| TypeReference::concrete_type_ref(&FullName::with_schema("pg_catalog", name), false);
    let (args, result, alternate) = match role {
        "input" => (vec![builtin("cstring")], base.clone(), true),
        "output" => (vec![base.clone()], builtin("cstring"), false),
        "receive" => (vec![builtin("internal")], base.clone(), true),
        "send" => (vec![base.clone()], builtin("bytea"), false),
        "typmod_in" => (
            vec![TypeReference::concrete_type_ref(
                &FullName::with_schema("pg_catalog", "cstring"),
                true,
            )],
            builtin("int4"),
            false,
        ),
        "typmod_out" => (vec![builtin("int4")], builtin("cstring"), false),
        "analyze" => (vec![builtin("internal")], builtin("bool"), false),
        "subscript" => (vec![builtin("internal")], builtin("internal"), false),
        _ => unreachable!("only recognized support roles reach signature lookup"),
    };
    let mut args = args.into_iter().map(Some).collect::<Vec<_>>();
    let exact = context.find_exact_function(routine, &args).or_else(|| {
        if !alternate {
            return None;
        }
        args.extend([Some(builtin("oid")), Some(builtin("int4"))]);
        context.find_exact_function(routine, &args)
    });
    if exact.is_some_and(|function| function.overload().ret_type().as_ref() == Some(&result)) {
        Ok(())
    } else {
        Err(invalid(&format!("invalid {role} function {routine}")))
    }
}

fn invalid(message: &str) -> AnalysisError {
    AnalysisError::unexpected_input_error(&format!("CREATE TYPE: {message}"))
}
