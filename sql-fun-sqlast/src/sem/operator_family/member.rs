//! Binding and validation shared by operator family and class declarations.

use std::sync::Arc;

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
        AnalysisError, OperatorIdentity, OwnershipIdentity, PgBuiltInType, TypeReference,
        ownership::RequestedIdentity,
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
                if declared_input == Some(slot.left.as_ref()) {
                    return;
                }
                context
                    .operator_class_for_family_type(family, &slot.left)
                    .map_or(MemberOwner::Loose, |class| {
                        MemberOwner::Class(Box::new(class.qualified_name()))
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
            left: Arc::new(left),
            right: Arc::new(right),
        },
        FamilyMember {
            object: MemberObject::Operator(operator.name().clone()),
            order_family: order_family.map(Box::new),
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
    let name = FullName::from_ast_name(syntax.name())?;
    let Some((left, right)) = support_types(syntax, method, &name, default_input, context)? else {
        return Ok(None);
    };
    let slot = FamilySlot {
        kind: MemberKind::Function,
        number,
        left: Arc::new(left),
        right: Arc::new(right),
    };
    Ok(Some((
        slot,
        FamilyMember {
            object: MemberObject::Function(name),
            order_family: None,
            owner,
        },
    )))
}

fn support_types(
    syntax: &OperatorClassFunction,
    method: &str,
    name: &FullName,
    default_input: Option<&TypeReference>,
    context: &mut impl ParseContext,
) -> Result<Option<(TypeReference, TypeReference)>, AnalysisError> {
    if let Some(types) = syntax.class_operands() {
        return resolve_family_types(types, context);
    }
    let positions = match (method, syntax.support()) {
        ("btree", 1) => (0, 1),
        ("btree", 3) => (0, 2),
        ("hash", 1 | 2) => (0, 0),
        _ => {
            if let Some(ty) = default_input {
                return Ok(Some((ty.clone(), ty.clone())));
            }
            return reject(
                context,
                "associated data types must be specified for index support function",
            );
        }
    };
    if let Some(types) = syntax.arguments() {
        if let (Some(left), Some(right)) = (types.get(positions.0), types.get(positions.1)) {
            let left = resolve_type(left, context)?;
            let right = resolve_type(right, context)?;
            return Ok(left.zip(right));
        }
    } else {
        // When the SQL omits the signature, a uniquely known SQL declaration
        // can still provide the types associated with this family slot.
        let requested =
            RequestedIdentity::new(&OwnershipIdentity::named(ObjectKind::FUNCTION, name), &[]);
        let lookup = requested.resolve_routine(context);
        if let Some(identity) = lookup.identity()
            && let Some(types) = identity.arguments()
            && let (Some(Some(left)), Some(Some(right))) =
                (types.get(positions.0), types.get(positions.1))
        {
            return Ok(Some((left.clone(), right.clone())));
        }
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

fn reject<T>(
    context: &mut impl ParseContext,
    message: impl Into<String>,
) -> Result<Option<T>, AnalysisError> {
    report_family_problem(context, message)?;
    Ok(None)
}
