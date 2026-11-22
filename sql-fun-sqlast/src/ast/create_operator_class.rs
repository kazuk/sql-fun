//! Unresolved PostgreSQL operator class declarations.

use crate::{
    ast::{Name, ObjectArgument, TryFromWithTokens, TypeName, postgres::required},
    error::AnalysisError,
    syn::{self, ListOpt, Opt, ScanToken},
};
use sql_fun_core::IVec;

#[allow(dead_code, reason = "retained for operator class semantic analysis")]
#[derive(Clone, Debug)]
pub(crate) struct CreateOperatorClass {
    name: Name,
    is_default: bool,
    input_type: TypeName,
    access_method: String,
    family: Option<Name>,
    members: Vec<OperatorClassMember>,
}

#[allow(dead_code, reason = "retained for operator class semantic analysis")]
#[derive(Clone, Debug)]
enum OperatorClassMember {
    Operator {
        strategy: i32,
        name: Name,
        operands: Option<Vec<ObjectArgument>>,
        order_family: Option<Name>,
    },
    Function {
        support: i32,
        name: Name,
        class_operands: Option<Vec<TypeName>>,
        arguments: Option<Vec<TypeName>>,
    },
    Storage(TypeName),
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

impl TryFromWithTokens<&syn::Node> for OperatorClassMember {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        // PostgreSQL parsenodes.h OPCLASS_ITEM_* constants.
        const OPERATOR: i32 = 1;
        const FUNCTION: i32 = 2;
        const STORAGE: i32 = 3;

        let member = required(
            value.as_create_op_class_item().as_inner(),
            "operator class member",
        )?;
        match member.get_itemtype() {
            OPERATOR => Self::operator(&member, tokens),
            FUNCTION => Self::function(&member, tokens),
            STORAGE => Self::storage(&member, tokens),
            _ => Err(AnalysisError::unexpected_input_error(
                "operator class member kind",
            )),
        }
    }
}

impl OperatorClassMember {
    fn operator(
        member: &syn::CreateOpClassItem,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let object = required(member.get_name().as_inner(), "operator class operator")?;
        Ok(Self::Operator {
            strategy: member.get_number(),
            name: Name::try_from(object.get_objname())?,
            operands: object_arguments(object.get_objargs(), tokens)?,
            order_family: optional_name(member.get_order_family())?,
        })
    }

    fn function(
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
        Ok(Self::Function {
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

    fn storage(
        member: &syn::CreateOpClassItem,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let typ = required(
            member.get_storedtype().as_inner(),
            "operator class storage type",
        )?;
        TypeName::try_from_with_tokens(&typ, tokens).map(Self::Storage)
    }
}

fn optional_name(value: syn::NodeListOpt) -> Result<Option<Name>, AnalysisError> {
    match value.as_inner() {
        Some(parts) if !parts.is_empty() => Name::try_from(value).map(Some),
        _ => Ok(None),
    }
}

fn type_names(
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{
        SqlFileSnapshot,
        ast::{Dialect, StatementKind, StatementNode, create_operator_class::OperatorClassMember},
        parse_statements::{parse_statements, split_postgres_statements},
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

        let [
            OperatorClassMember::Operator {
                strategy: 1,
                name: search,
                operands: None,
                order_family: None,
            },
            OperatorClassMember::Operator {
                strategy: 2,
                name: ordering,
                operands: Some(operands),
                order_family: Some(order_family),
            },
            OperatorClassMember::Function {
                support: 3,
                name: support,
                class_operands: Some(class_operands),
                arguments: Some(arguments),
            },
            OperatorClassMember::Function {
                support: 4,
                class_operands: None,
                arguments: Some(empty),
                ..
            },
            OperatorClassMember::Function {
                support: 5,
                class_operands: None,
                arguments: None,
                ..
            },
            OperatorClassMember::Storage(storage),
        ] = class.members.as_slice()
        else {
            panic!("unexpected member order or structure: {:?}", class.members);
        };
        assert_eq!(search.0, ["="]);
        assert_eq!(ordering.0, ["App", "!!"]);
        assert_eq!(order_family.0, ["pg_catalog", "integer_ops"]);
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
        assert_eq!(support.0, ["App", "support"]);
        assert_eq!(class_operands[0].name.0, ["pg_catalog", "int8"]);
        assert_eq!(class_operands[1].name.0, ["pg_catalog", "int2"]);
        assert_eq!(arguments.len(), 2);
        assert_eq!(arguments[0].name.0, ["internal"]);
        assert_eq!(arguments[1].name.0, ["pg_catalog", "int4"]);
        assert!(empty.is_empty());
        assert_eq!(storage.name.0, ["bytea"]);
    }

    #[test]
    fn converts_operator_classes_from_bundled_extension_paths() {
        for version in ["17", "18"] {
            let extension_dir = Path::new(env!("SQL_FUN_HOME"))
                .join("postgres")
                .join(version)
                .join("extension");
            for (script, expected_count) in [
                ("btree_gin--1.0.sql", 23),
                ("btree_gin--1.0--1.1.sql", 1),
                ("btree_gin--1.1--1.2.sql", 1),
                ("btree_gin--1.2--1.3.sql", 4),
                ("pg_trgm--1.3.sql", 2),
            ] {
                let source = SqlFileSnapshot::read(&extension_dir.join(script))
                    .unwrap()
                    .postgres_text();
                let mut names = Vec::new();
                for sql in split_postgres_statements(&source).unwrap() {
                    // Select only operator classes from scripts that also contain other DDL.
                    let Some(start) = sql.find("CREATE OPERATOR CLASS ") else {
                        continue;
                    };
                    let statement = parse_statements(&sql[start..], Dialect::postgres())
                        .next()
                        .unwrap()
                        .unwrap_or_else(|error| panic!("{version}/{script}: {error}: {sql}"));
                    let StatementNode::CreateOperatorClass(class) = statement.node() else {
                        panic!("expected CREATE OPERATOR CLASS in {version}/{script}");
                    };
                    names.push(class.name.0.join("."));
                }
                assert_eq!(names.len(), expected_count, "{version}/{script}: {names:?}");
                if script == "btree_gin--1.0.sql" {
                    assert_eq!(names[0], "int2_ops");
                } else if script == "pg_trgm--1.3.sql" {
                    assert_eq!(names[0], "gist_trgm_ops");
                }
            }
        }
    }
}
