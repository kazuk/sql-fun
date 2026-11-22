//! Unresolved PostgreSQL operator class declarations.

use crate::{
    ast::{Name, ObjectArgument, TryFromWithTokens, TypeName, postgres::required},
    error::AnalysisError,
    syn::{self, ListOpt, Opt, ScanToken},
};
use ::sql_fun_core::IVec;

#[derive(Clone, Debug)]
pub(crate) struct CreateOperatorClass {
    name: Name,
    is_default: bool,
    input_type: TypeName,
    access_method: String,
    family: Option<Name>,
    members: Vec<OperatorClassMember>,
}

#[derive(Clone, Debug)]
pub(crate) enum OperatorClassMember {
    Operator(OperatorClassOperator),
    Function(OperatorClassFunction),
    Storage(TypeName),
}

#[derive(Clone, Debug)]
pub(crate) struct OperatorClassOperator {
    strategy: i32,
    name: Name,
    operands: Option<Vec<ObjectArgument>>,
    order_family: Option<Name>,
}

#[derive(Clone, Debug)]
pub(crate) struct OperatorClassFunction {
    support: i32,
    name: Name,
    class_operands: Option<Vec<TypeName>>,
    arguments: Option<Vec<TypeName>>,
}

impl CreateOperatorClass {
    pub(crate) fn name(&self) -> &Name {
        &self.name
    }

    pub(crate) const fn is_default(&self) -> bool {
        self.is_default
    }

    pub(crate) fn input_type(&self) -> &TypeName {
        &self.input_type
    }

    pub(crate) fn access_method(&self) -> &str {
        &self.access_method
    }

    pub(crate) fn family(&self) -> Option<&Name> {
        self.family.as_ref()
    }

    pub(crate) fn members(&self) -> &[OperatorClassMember] {
        &self.members
    }
}

impl OperatorClassOperator {
    pub(crate) const fn strategy(&self) -> i32 {
        self.strategy
    }

    pub(crate) fn name(&self) -> &Name {
        &self.name
    }

    pub(crate) fn operands(&self) -> Option<&[ObjectArgument]> {
        self.operands.as_deref()
    }

    pub(crate) fn order_family(&self) -> Option<&Name> {
        self.order_family.as_ref()
    }
}

impl OperatorClassFunction {
    pub(crate) const fn support(&self) -> i32 {
        self.support
    }

    pub(crate) fn name(&self) -> &Name {
        &self.name
    }

    pub(crate) fn class_operands(&self) -> Option<&[TypeName]> {
        self.class_operands.as_deref()
    }

    pub(crate) fn arguments(&self) -> Option<&[TypeName]> {
        self.arguments.as_deref()
    }
}

#[cfg(test)]
impl OperatorClassMember {
    fn as_operator(&self) -> Option<&OperatorClassOperator> {
        if let Self::Operator(operator) = self {
            Some(operator)
        } else {
            None
        }
    }

    fn as_function(&self) -> Option<&OperatorClassFunction> {
        if let Self::Function(function) = self {
            Some(function)
        } else {
            None
        }
    }

    fn as_storage(&self) -> Option<&TypeName> {
        if let Self::Storage(typ) = self {
            Some(typ)
        } else {
            None
        }
    }
}

impl TryFromWithTokens<&syn::CreateOpClassStmt> for CreateOperatorClass {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::CreateOpClassStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let input = required(value.get_datatype().as_inner(), "operator class input type")?;
        let members = required(value.get_items().as_inner(), "operator class members")?;
        Ok(Self {
            name: Name::try_from(value.get_opclassname())?,
            is_default: value.get_is_default(),
            input_type: TypeName::try_from_with_tokens(&input, tokens)?,
            access_method: value.get_amname(),
            family: optional_name(value.get_opfamilyname())?,
            members: members
                .iter()
                .map(|member| OperatorClassMember::try_from_with_tokens(member, tokens))
                .collect::<Result<_, _>>()?,
        })
    }
}

#[cfg(test)]
mod test_create_operator_class_conversion {
    use crate::{
        ast::{Dialect, StatementKind, StatementNode},
        parse_statements,
    };

    #[test]
    fn preserves_operator_class_structure_and_type_locations() {
        let sql = r#"SELECT 1;
CREATE OPERATOR CLASS "App".ops DEFAULT FOR TYPE int4 USING gist FAMILY "App".family AS
    OPERATOR 1 = FOR SEARCH,
    OPERATOR 2 "App".!! (NONE, int4) FOR ORDER BY pg_catalog.integer_ops,
    FUNCTION 3 (bigint, smallint) "App".support (internal, integer),
    FUNCTION 4 "App".empty (),
    FUNCTION 5 "App".unspecified,
    STORAGE bytea;"#;
        let statement = parse_statements(sql, Dialect::postgres())
            .nth(1)
            .unwrap()
            .unwrap();
        assert_eq!(statement.kind(), StatementKind::CreateOperatorClass);
        let StatementNode::CreateOperatorClass(class) = statement.node() else {
            panic!("expected CREATE OPERATOR CLASS");
        };
        assert_eq!(class.name.0, ["App", "ops"]);
        assert!(class.is_default);
        assert_eq!(class.access_method, "gist");
        assert_eq!(class.family.as_ref().unwrap().0, ["App", "family"]);

        let statement_sql = statement.span().get_from(sql).unwrap();
        assert!(
            statement_sql
                .trim_start()
                .starts_with("CREATE OPERATOR CLASS")
        );
        assert_eq!(
            class.input_type.location.unwrap().get_from(statement_sql),
            Some("int4")
        );

        assert_eq!(class.members.len(), 6);
        let search = class.members[0].as_operator().unwrap();
        assert_eq!(search.strategy, 1);
        assert_eq!(search.name.0, ["="]);
        assert!(search.operands.is_none());
        assert!(search.order_family.is_none());

        let ordering = class.members[1].as_operator().unwrap();
        assert_eq!(ordering.strategy, 2);
        assert_eq!(ordering.name.0, ["App", "!!"]);
        assert_eq!(
            ordering.order_family.as_ref().unwrap().0,
            ["pg_catalog", "integer_ops"]
        );
        let operands = ordering.operands.as_ref().unwrap();
        assert_eq!(operands.len(), 2);
        assert!(operands[0].type_name().is_none());
        assert_eq!(
            operands[1]
                .type_name()
                .as_ref()
                .unwrap()
                .location
                .unwrap()
                .get_from(statement_sql),
            Some("int4")
        );
        let support = class.members[2].as_function().unwrap();
        assert_eq!(support.support, 3);
        assert_eq!(support.name.0, ["App", "support"]);
        let class_operands = support.class_operands.as_ref().unwrap();
        assert_eq!(class_operands[0].name.0, ["pg_catalog", "int8"]);
        assert_eq!(class_operands[1].name.0, ["pg_catalog", "int2"]);
        let arguments = support.arguments.as_ref().unwrap();
        assert_eq!(arguments.len(), 2);
        assert_eq!(arguments[0].name.0, ["internal"]);
        assert_eq!(arguments[1].name.0, ["pg_catalog", "int4"]);

        let empty = class.members[3].as_function().unwrap();
        assert_eq!(empty.support, 4);
        assert!(empty.class_operands.is_none());
        assert!(empty.arguments.as_ref().unwrap().is_empty());

        let unspecified = class.members[4].as_function().unwrap();
        assert_eq!(unspecified.support, 5);
        assert!(unspecified.class_operands.is_none());
        assert!(unspecified.arguments.is_none());
        assert_eq!(class.members[5].as_storage().unwrap().name.0, ["bytea"]);
    }
}

pub(crate) enum OperatorClassMemberKind {
    Operator,
    Function,
    Storage,
}

impl TryFrom<i32> for OperatorClassMemberKind {
    type Error = AnalysisError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        // PostgreSQL parsenodes.h OPCLASS_ITEM_* constants.
        match value {
            1 => Ok(Self::Operator),
            2 => Ok(Self::Function),
            3 => Ok(Self::Storage),
            _ => AnalysisError::raise_unexpected_input("operator class member kind"),
        }
    }
}

impl TryFromWithTokens<&syn::Node> for OperatorClassMember {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let member = required(
            value.as_create_op_class_item().as_inner(),
            "operator class member",
        )?;
        match OperatorClassMemberKind::try_from(member.get_itemtype())? {
            OperatorClassMemberKind::Operator => {
                OperatorClassOperator::try_from_with_tokens(&member, tokens).map(Self::Operator)
            }
            OperatorClassMemberKind::Function => {
                OperatorClassFunction::try_from_with_tokens(&member, tokens).map(Self::Function)
            }
            OperatorClassMemberKind::Storage => {
                let typ = required(
                    member.get_storedtype().as_inner(),
                    "operator class storage type",
                )?;
                TypeName::try_from_with_tokens(&typ, tokens).map(Self::Storage)
            }
        }
    }
}

impl TryFromWithTokens<&syn::CreateOpClassItem> for OperatorClassOperator {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        member: &syn::CreateOpClassItem,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let object = required(member.get_name().as_inner(), "operator class operator")?;
        Ok(Self {
            strategy: member.get_number(),
            name: Name::try_from(object.get_objname())?,
            operands: object_arguments(object.get_objargs(), tokens)?,
            order_family: optional_name(member.get_order_family())?,
        })
    }
}

impl TryFromWithTokens<&syn::CreateOpClassItem> for OperatorClassFunction {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        member: &syn::CreateOpClassItem,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let object = required(
            member.get_name().as_inner(),
            "operator class support function",
        )?;
        let arguments = if object.get_args_unspecified() {
            None
        } else {
            Some(type_names(object.get_objargs(), tokens)?)
        };
        let class_operands = type_names(member.get_class_args(), tokens)?;
        Ok(Self {
            support: member.get_number(),
            name: Name::try_from(object.get_objname())?,
            class_operands: if class_operands.is_empty() {
                None
            } else {
                Some(class_operands)
            },
            arguments,
        })
    }
}

fn optional_name(value: syn::NodeListOpt) -> Result<Option<Name>, AnalysisError> {
    match value.as_inner() {
        Some(parts) if !parts.is_empty() => Name::try_from(value).map(Some),
        _ => Ok(None),
    }
}

pub(crate) fn type_names(
    value: syn::NodeListOpt,
    tokens: &IVec<ScanToken>,
) -> Result<Vec<TypeName>, AnalysisError> {
    value
        .as_inner()
        .unwrap_or_default()
        .iter()
        .map(|node| {
            let typ = required(node.as_type_name().as_inner(), "operator class type")?;
            TypeName::try_from_with_tokens(&typ, tokens)
        })
        .collect()
}

fn object_arguments(
    value: syn::NodeListOpt,
    tokens: &IVec<ScanToken>,
) -> Result<Option<Vec<ObjectArgument>>, AnalysisError> {
    value
        .as_inner()
        .filter(|arguments| !arguments.is_empty())
        .map(|arguments| {
            arguments
                .iter()
                .map(|argument| ObjectArgument::try_from_node(argument, tokens))
                .collect::<Result<_, _>>()
        })
        .transpose()
}
