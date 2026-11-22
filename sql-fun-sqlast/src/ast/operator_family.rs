//! Unresolved PostgreSQL operator family declarations and member changes.

use ::sql_fun_core::IVec;

use crate::{
    ast::{
        Name, TryFromWithTokens, TypeName,
        create_operator_class::{OperatorClassMember, OperatorClassMemberKind, type_names},
        postgres::required,
    },
    error::AnalysisError,
    syn::{self, ListOpt, Opt, ScanToken},
};

#[derive(Clone, Debug)]
pub(crate) struct CreateOperatorFamily {
    name: Name,
    access_method: String,
}

impl CreateOperatorFamily {
    pub(crate) const fn name(&self) -> &Name {
        &self.name
    }

    pub(crate) fn access_method(&self) -> &str {
        &self.access_method
    }
}

impl TryFrom<&syn::CreateOpFamilyStmt> for CreateOperatorFamily {
    type Error = AnalysisError;

    fn try_from(value: &syn::CreateOpFamilyStmt) -> Result<Self, Self::Error> {
        Ok(Self {
            name: Name::try_from(value.get_opfamilyname())?,
            access_method: value.get_amname(),
        })
    }
}

#[cfg(test)]
mod test_create_operator_family {
    use crate::{
        ast::{Dialect, StatementKind, StatementNode},
        parse_statements,
    };

    #[test]
    fn retains_qualified_name_and_method() {
        let sql = r#"CREATE OPERATOR FAMILY "App".ops USING "Custom";"#;
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(statement.kind(), StatementKind::CreateOperatorFamily);
        let StatementNode::CreateOperatorFamily(family) = statement.node() else {
            panic!("expected CREATE OPERATOR FAMILY");
        };
        assert_eq!(family.name().parts(), ["App", "ops"]);
        assert_eq!(family.access_method(), "Custom");
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AlterOperatorFamily {
    name: Name,
    access_method: String,
    action: AlterOperatorFamilyAction,
}

impl AlterOperatorFamily {
    pub(crate) const fn name(&self) -> &Name {
        &self.name
    }

    pub(crate) fn access_method(&self) -> &str {
        &self.access_method
    }

    pub(crate) const fn action(&self) -> &AlterOperatorFamilyAction {
        &self.action
    }
}

impl TryFromWithTokens<&syn::AlterOpFamilyStmt> for AlterOperatorFamily {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::AlterOpFamilyStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let items = required(value.get_items().as_inner(), "operator family members")?;
        let action = if value.get_is_drop() {
            AlterOperatorFamilyAction::Drop(
                items
                    .iter()
                    .map(|item| OperatorFamilyDropMember::try_from_with_tokens(item, tokens))
                    .collect::<Result<_, _>>()?,
            )
        } else {
            AlterOperatorFamilyAction::Add(
                items
                    .iter()
                    .map(|item| OperatorClassMember::try_from_with_tokens(item, tokens))
                    .collect::<Result<_, _>>()?,
            )
        };
        Ok(Self {
            name: Name::try_from(value.get_opfamilyname())?,
            access_method: value.get_amname(),
            action,
        })
    }
}

#[cfg(test)]
mod test_alter_operator_family {
    use crate::{
        ast::{Dialect, StatementKind, StatementNode, create_operator_class::OperatorClassMember},
        parse_statements,
    };

    use crate::ast::operator_family::{AlterOperatorFamilyAction, OperatorFamilyMemberKind};

    #[test]
    fn retains_added_operator_and_support_signatures() {
        let sql = r#"ALTER OPERATOR FAMILY "App".ops USING btree ADD
            OPERATOR 3 = (integer, integer) FOR ORDER BY pg_catalog.integer_ops,
            FUNCTION 1 (integer) pg_catalog.btint4cmp(integer, integer);"#;
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(statement.kind(), StatementKind::AlterOperatorFamily);
        let StatementNode::AlterOperatorFamily(family) = statement.node() else {
            panic!("expected ALTER OPERATOR FAMILY");
        };
        assert_eq!(family.name().parts(), ["App", "ops"]);
        assert_eq!(family.access_method(), "btree");
        let AlterOperatorFamilyAction::Add(members) = family.action() else {
            panic!("expected ADD");
        };
        let [
            OperatorClassMember::Operator(operator),
            OperatorClassMember::Function(function),
        ] = members.as_ref()
        else {
            panic!("expected operator and support members");
        };
        assert_eq!(operator.strategy(), 3);
        assert_eq!(operator.name().parts(), ["="]);
        assert_eq!(operator.operands().unwrap().len(), 2);
        assert_eq!(
            operator.order_family().unwrap().parts(),
            ["pg_catalog", "integer_ops"]
        );
        assert_eq!(function.support(), 1);
        assert_eq!(function.name().parts(), ["pg_catalog", "btint4cmp"]);
        assert_eq!(function.class_operands().unwrap().len(), 1);
        assert_eq!(function.arguments().unwrap().len(), 2);
    }

    #[test]
    fn retains_drop_slots_and_statement_relative_type_locations() {
        let sql = r#"SELECT 1;
            ALTER OPERATOR FAMILY "App".ops USING btree
            DROP OPERATOR 3 (int4), FUNCTION 1 (int4, int2);"#;
        let statement = parse_statements(sql, Dialect::postgres())
            .nth(1)
            .unwrap()
            .unwrap();
        let StatementNode::AlterOperatorFamily(family) = statement.node() else {
            panic!("expected ALTER OPERATOR FAMILY");
        };
        let AlterOperatorFamilyAction::Drop(members) = family.action() else {
            panic!("expected DROP");
        };
        let [operator, function] = members.as_ref() else {
            panic!("expected two drop slots");
        };
        assert_eq!(operator.kind(), OperatorFamilyMemberKind::Operator);
        assert_eq!(operator.number(), 3);
        assert_eq!(operator.operands().len(), 1);
        assert_eq!(function.kind(), OperatorFamilyMemberKind::Function);
        assert_eq!(function.number(), 1);
        assert_eq!(function.operands().len(), 2);
        assert_eq!(
            function.operands()[1]
                .location
                .unwrap()
                .get_from(statement.span().get_from(sql).unwrap()),
            Some("int2")
        );
    }
}

#[derive(Clone, Debug)]
pub(crate) enum AlterOperatorFamilyAction {
    Add(Box<[OperatorClassMember]>),
    Drop(Box<[OperatorFamilyDropMember]>),
}

#[derive(Clone, Debug)]
pub(crate) struct OperatorFamilyDropMember {
    kind: OperatorFamilyMemberKind,
    number: i32,
    operands: Vec<TypeName>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OperatorFamilyMemberKind {
    Operator,
    Function,
}

impl OperatorFamilyDropMember {
    pub(crate) const fn kind(&self) -> OperatorFamilyMemberKind {
        self.kind
    }

    pub(crate) const fn number(&self) -> i32 {
        self.number
    }

    pub(crate) fn operands(&self) -> &[TypeName] {
        &self.operands
    }
}

impl TryFromWithTokens<&syn::Node> for OperatorFamilyDropMember {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let item = required(
            value.as_create_op_class_item().as_inner(),
            "operator family drop member",
        )?;
        let kind = match OperatorClassMemberKind::try_from(item.get_itemtype())? {
            OperatorClassMemberKind::Operator => OperatorFamilyMemberKind::Operator,
            OperatorClassMemberKind::Function => OperatorFamilyMemberKind::Function,
            OperatorClassMemberKind::Storage => {
                return AnalysisError::raise_unexpected_input("operator family drop member kind");
            }
        };
        Ok(Self {
            kind,
            number: item.get_number(),
            operands: type_names(item.get_class_args(), tokens)?,
        })
    }
}
