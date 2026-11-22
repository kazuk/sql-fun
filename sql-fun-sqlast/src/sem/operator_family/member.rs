//! Binding and validation shared by operator family and class declarations.

use crate::{
    ast::{
        ObjectArgument, ObjectKind, TypeName,
        create_operator_class::{
            OperatorClassFunction, OperatorClassMember, OperatorClassOperator,
        },
    },
    context::ParseContext,
    names::{FullName, OperatorName},
    sem::{
        AnalysisError, ExactFunction, OperatorIdentity, OwnershipIdentity, OwnershipLookup,
        PgBuiltInType, TypeReference, ownership::RequestedIdentity,
    },
};

use crate::sem::operator_family::{
    FamilyMember, FamilySlot, MemberKind, MemberObject, MemberOwner, OperatorFamilyKey,
    report_family_problem,
};

pub(crate) fn resolve_family_member(
    member: &OperatorClassMember,
    family: &OperatorFamilyKey,
    default_input: Option<&TypeReference>,
    owner: MemberOwner,
    context: &mut impl ParseContext,
) -> Result<Option<(FamilySlot, FamilyMember)>, AnalysisError> {
    let method = &family.method;
    let resolved = match member {
        OperatorClassMember::Operator(operator) => {
            resolve_operator(operator, method, default_input, owner, context)?
        }
        OperatorClassMember::Function(function) => {
            resolve_function(function, method, default_input, owner, context)?
        }
        OperatorClassMember::Storage(_) => {
            return reject(
                context,
                "STORAGE cannot be specified in ALTER OPERATOR FAMILY",
            );
        }
    };
    Ok(resolved.map(|(slot, mut member)| {
        member.adjust_owner(family, &slot, default_input, context);
        (slot, member)
    }))
}

impl FamilyMember {
    fn adjust_owner(
        &mut self,
        family: &OperatorFamilyKey,
        slot: &FamilySlot,
        declared_input: Option<&TypeReference>,
        context: &impl ParseContext,
    ) {
        self.owner = match family.method.as_str() {
            "btree" | "hash"
                if slot.left == slot.right
                    && (slot.kind == MemberKind::Operator || slot.number == 1) =>
            {
                if declared_input == Some(&slot.left) {
                    return;
                }
                context
                    .operator_class_for_family_type(family, &slot.left)
                    .map_or(MemberOwner::Loose, |class| {
                        MemberOwner::Class(class.qualified_name())
                    })
            }
            "btree" | "hash" => MemberOwner::Loose,
            "gist" | "spgist" | "gin" if !slot.is_required_support(&family.method) => {
                MemberOwner::Loose
            }
            "gist" | "spgist" | "gin" if self.owner == MemberOwner::Loose => MemberOwner::Family,
            _ => return,
        };
    }
}

impl FamilySlot {
    fn is_required_support(&self, method: &str) -> bool {
        if self.kind != MemberKind::Function {
            return false;
        }
        match method {
            "gist" => matches!(self.number, 1 | 2 | 5..=7),
            "spgist" => matches!(self.number, 1..=5),
            "gin" => matches!(self.number, 2 | 3),
            _ => false,
        }
    }
}

fn resolve_type(
    ty: &TypeName,
    context: &mut impl ParseContext,
) -> Result<Option<TypeReference>, AnalysisError> {
    match OwnershipIdentity::argument_type_from_ast(ty, context)? {
        Some(ty) => Ok(Some(ty)),
        None => reject(
            context,
            format!(
                "type {} does not exist",
                FullName::from_ast_name(ty.name())?
            ),
        ),
    }
}

pub(crate) fn resolve_family_types(
    types: &[TypeName],
    context: &mut impl ParseContext,
) -> Result<Option<(TypeReference, TypeReference)>, AnalysisError> {
    match types {
        [left] => Ok(resolve_type(left, context)?.map(|left| (left.clone(), left))),
        [left, right] => {
            let left = resolve_type(left, context)?;
            let right = resolve_type(right, context)?;
            Ok(left.zip(right))
        }
        _ => reject(context, "one or two associated types must be specified"),
    }
}

fn resolve_operator_types(
    operands: Option<&[ObjectArgument]>,
    default_input: Option<&TypeReference>,
    context: &mut impl ParseContext,
) -> Result<Option<(TypeReference, TypeReference)>, AnalysisError> {
    match operands {
        Some([left, right]) => match (left.type_name(), right.type_name()) {
            (Some(left), Some(right)) => {
                let left = resolve_type(left, context)?;
                let right = resolve_type(right, context)?;
                Ok(left.zip(right))
            }
            _ => reject(context, "index operators must be binary"),
        },
        Some(_) => reject(context, "index operators must be binary"),
        None => match default_input {
            Some(ty) => Ok(Some((ty.clone(), ty.clone()))),
            None => reject(
                context,
                "operator argument types must be specified in ALTER OPERATOR FAMILY",
            ),
        },
    }
}

fn resolve_operator(
    syntax: &OperatorClassOperator,
    method: &str,
    default_input: Option<&TypeReference>,
    owner: MemberOwner,
    context: &mut impl ParseContext,
) -> Result<Option<(FamilySlot, FamilyMember)>, AnalysisError> {
    let number = syntax.strategy();
    if !validate_slot_number(MemberKind::Operator, number, method, context)? {
        return Ok(None);
    }
    let Some((left, right)) = resolve_operator_types(syntax.operands(), default_input, context)?
    else {
        return Ok(None);
    };
    let name = FullName::from_ast_name(syntax.name())?;
    let Some((operator, returns)) = OperatorIdentity::resolve_binary(
        &OperatorName::from_full_name(&name),
        &left,
        &right,
        context,
    ) else {
        return reject(
            context,
            format!(
                "operator {}({left}, {right}) does not exist",
                name.name().as_str()
            ),
        );
    };
    let order_family = if let Some(family) = syntax.order_family() {
        let Some(key) = OperatorFamilyKey::resolve(family, "btree", context)? else {
            return reject(
                context,
                format!(
                    "B-tree ordering family {} does not exist",
                    FullName::from_ast_name(family)?
                ),
            );
        };
        if matches!(method, "btree" | "hash" | "gin" | "brin") {
            return reject(
                context,
                format!("access method {method:?} does not support ordering operators"),
            );
        }
        Some(key)
    } else {
        if returns != PgBuiltInType::bool() {
            return reject(context, "index search operators must return boolean");
        }
        None
    };
    Ok(Some((
        FamilySlot {
            kind: MemberKind::Operator,
            number,
            left,
            right,
        },
        FamilyMember {
            object: MemberObject::Operator(operator),
            order_family,
            owner,
        },
    )))
}

fn resolve_function(
    syntax: &OperatorClassFunction,
    method: &str,
    default_input: Option<&TypeReference>,
    owner: MemberOwner,
    context: &mut impl ParseContext,
) -> Result<Option<(FamilySlot, FamilyMember)>, AnalysisError> {
    let number = syntax.support();
    if !validate_slot_number(MemberKind::Function, number, method, context)? {
        return Ok(None);
    }
    let Some(function) = exact_support_function(syntax, context)? else {
        return Ok(None);
    };
    let Some(arguments) = function
        .overload()
        .identity_arguments()
        .into_iter()
        .collect::<Option<Vec<_>>>()
    else {
        return reject(
            context,
            "index support function has an unresolved argument type",
        );
    };
    let Some((left, right)) = support_types(syntax, method, &arguments, default_input, context)?
    else {
        return Ok(None);
    };
    let slot = FamilySlot {
        kind: MemberKind::Function,
        number,
        left,
        right,
    };
    if !validate_support_signature(method, &slot, &arguments, &function, default_input, context)? {
        return Ok(None);
    }
    Ok(Some((
        slot,
        FamilyMember {
            object: MemberObject::Function {
                name: function.name().clone(),
                arguments,
            },
            order_family: None,
            owner,
        },
    )))
}

fn exact_support_function(
    syntax: &OperatorClassFunction,
    context: &mut impl ParseContext,
) -> Result<Option<ExactFunction>, AnalysisError> {
    let name = FullName::from_ast_name(syntax.name())?;
    if let Some(types) = syntax.arguments() {
        let mut arguments = Vec::with_capacity(types.len());
        for ty in types {
            let Some(ty) = resolve_type(ty, context)? else {
                return Ok(None);
            };
            arguments.push(Some(ty));
        }
        if let Some(function) = context.find_exact_function(&name, &arguments) {
            return Ok(Some(function));
        }
        return reject(
            context,
            format!("support function {name} with the declared argument types does not exist"),
        );
    }
    let requested =
        RequestedIdentity::new(&OwnershipIdentity::named(ObjectKind::FUNCTION, &name), &[]);
    match requested.resolve_routine(context) {
        OwnershipLookup::Resolved(identity) => {
            if let Some(arguments) = identity.arguments()
                && let Some(function) = context.find_exact_function(identity.name(), arguments)
            {
                return Ok(Some(function));
            }
            reject(context, format!("support function {name} does not exist"))
        }
        OwnershipLookup::Ambiguous(_) => reject(
            context,
            format!("support function name {name} is not unique"),
        ),
        _ => reject(context, format!("support function {name} does not exist")),
    }
}

fn support_types(
    syntax: &OperatorClassFunction,
    method: &str,
    arguments: &[TypeReference],
    default_input: Option<&TypeReference>,
    context: &mut impl ParseContext,
) -> Result<Option<(TypeReference, TypeReference)>, AnalysisError> {
    if let Some(types) = syntax.class_operands() {
        return resolve_family_types(types, context);
    }
    let inferred = match (method, syntax.support(), arguments) {
        ("btree", 1, [left, right]) => Some((left, right)),
        ("btree", 3, [left, _, offset, _, _]) => Some((left, offset)),
        ("hash", 1 | 2, [left, ..]) => Some((left, left)),
        _ => default_input.map(|ty| (ty, ty)),
    };
    if let Some((left, right)) = inferred {
        return Ok(Some((left.clone(), right.clone())));
    }
    reject(
        context,
        "associated data types must be specified for index support function",
    )
}

pub(crate) fn validate_slot_number(
    kind: MemberKind,
    number: i32,
    method: &str,
    context: &mut impl ParseContext,
) -> Result<bool, AnalysisError> {
    // PostgreSQL's SQL-level bounds for builtin index methods. External
    // handlers are opaque here, so check only the catalog-wide int16 bound.
    let max = match (kind, method) {
        (MemberKind::Operator, "btree") => 5,
        (MemberKind::Operator, "hash") => 1,
        (MemberKind::Function, "btree") => {
            if postgres18(context) {
                6
            } else {
                5
            }
        }
        (MemberKind::Function, "hash") => 3,
        (MemberKind::Function, "gist") => {
            if postgres18(context) {
                12
            } else {
                11
            }
        }
        (MemberKind::Function, "gin" | "spgist") => 7,
        (MemberKind::Function, "brin") => 15,
        _ => i32::from(i16::MAX),
    };
    if (1..=max).contains(&number) {
        return Ok(true);
    }
    let label = match kind {
        MemberKind::Operator => "operator",
        MemberKind::Function => "function",
    };
    report_family_problem(
        context,
        format!("invalid {label} number {number}, must be between 1 and {max}"),
    )?;
    Ok(false)
}

fn postgres18(context: &impl ParseContext) -> bool {
    let Ok(configured) = context.get_args().postgres_version(context.metadata()) else {
        return false;
    };
    configured
        .to_string()
        .parse::<::sql_fun_core::PostgresVersion>()
        .is_ok_and(|version| version.major() >= 18)
}

fn validate_support_signature(
    method: &str,
    slot: &FamilySlot,
    args: &[TypeReference],
    function: &ExactFunction,
    declared_input: Option<&TypeReference>,
    context: &mut impl ParseContext,
) -> Result<bool, AnalysisError> {
    let builtin =
        |name| TypeReference::concrete_type_ref(&FullName::with_schema("pg_catalog", name), false);
    let returns = function.overload().ret_type();
    let number = slot.number;
    let same_type = slot.left == slot.right;
    if matches!(
        (method, number),
        ("btree" | "brin", 5) | ("hash", 3) | ("gist", 10) | ("gin" | "spgist", 7)
    ) {
        if !same_type || declared_input.is_some_and(|ty| ty != &slot.left) {
            report_family_problem(
                context,
                "associated data types for index options support must match the class input type, when declared, and each other",
            )?;
            return Ok(false);
        }
        if args != [builtin("internal")] || returns.as_ref() != Some(&builtin("void")) {
            report_family_problem(
                context,
                "index options support functions must accept internal and return void",
            )?;
            return Ok(false);
        }
        return Ok(true);
    }
    let problem = match (method, number) {
        ("btree", 1) if args.len() != 2 || returns.as_ref() != Some(&PgBuiltInType::int4()) => {
            "ordering comparison functions must take two arguments and return integer"
        }
        ("btree", 3) if args.len() != 5 || returns.as_ref() != Some(&PgBuiltInType::bool()) => {
            "ordering in_range functions must take five arguments and return boolean"
        }
        ("btree", 4)
            if args.len() != 1
                || returns.as_ref() != Some(&PgBuiltInType::bool())
                || !same_type =>
        {
            "ordering equal image functions must take one argument, return boolean, and support a single type"
        }
        ("btree", 2 | 6)
            if args != [builtin("internal")] || returns.as_ref() != Some(&builtin("void")) =>
        {
            "ordering sort and skip support functions must accept internal and return void"
        }
        ("btree", 6) if !same_type => "ordering skip support functions must support a single type",
        ("hash", 1) if args.len() != 1 || returns.as_ref() != Some(&PgBuiltInType::int4()) => {
            "hash function 1 must take one argument and return integer"
        }
        ("hash", 2) if args.len() != 2 || returns.as_ref() != Some(&PgBuiltInType::int8()) => {
            "hash function 2 must take two arguments and return bigint"
        }
        _ => return Ok(true),
    };
    report_family_problem(context, problem)?;
    Ok(false)
}

fn reject<T>(
    context: &mut impl ParseContext,
    message: impl Into<String>,
) -> Result<Option<T>, AnalysisError> {
    report_family_problem(context, message)?;
    Ok(None)
}
