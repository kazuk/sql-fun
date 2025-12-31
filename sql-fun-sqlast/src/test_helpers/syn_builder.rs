use crate::{
    sem::FullName,
    syn::{
        AConst, AConstInner, Alias, AliasBuilder, AliasOpt, AlterTableCmd, AlterTableCmdBuilder,
        AlterTableType, ConstrType, Constraint, ConstraintBuilder, DropBehaviorOpt,
        FunctionParameter, FunctionParameterBuilder, FunctionParameterMode, IntegerBuilder,
        IntoClauseOpt, LimitOptionOpt, Node, NodeInner, NodeList, RangeSubselect,
        RangeSubselectBuilder, RangeVarOpt, ResTarget, RoleSpecOpt, SelectStmt, SelectStmtOpt,
        SetOperation, StringBuilder, TypeName, TypeNameOpt, WithClauseOpt,
    },
};

/// syntax node builder
pub struct SynBuilder {}

impl SynBuilder {
    /// create builder instance
    pub fn new() -> Self {
        Self {}
    }

    /// create [crate::syn::String] value
    pub fn string(&self, value: &str) -> crate::syn::String {
        crate::syn::String::from(
            StringBuilder::default()
                .sval(String::from(value))
                .build()
                .unwrap(),
        )
    }

    /// wrap [create::syn::String] to [NodeInner::String] variant
    pub fn as_string_node(&self, string: crate::syn::String) -> Node {
        Node::from(NodeInner::from(string))
    }

    /// convert [Node] slice to [NodeList]
    pub fn as_node_list(&self, items: &[Node]) -> NodeList {
        NodeList::from(items.to_vec())
    }

    /// create empty [NodeList]
    pub fn empty_node_list(&self) -> NodeList {
        NodeList::default()
    }

    /// create [TypeName]
    pub fn type_name(&self, name_items: &[&str]) -> TypeName {
        let mut names = Vec::new();
        for name_item in name_items {
            let name_item = self.string(name_item);
            names.push(self.as_string_node(name_item))
        }
        let names = self.as_node_list(&names);

        let type_name = crate::syn::TypeNameBuilder::default()
            .names(names)
            .type_oid(0)
            .setof(false)
            .pct_type(false)
            .typmods(self.empty_node_list())
            .typemod(0)
            .array_bounds(self.empty_node_list())
            .location(0)
            .build()
            .unwrap();
        TypeName::from(type_name)
    }

    /// create [crate::syn::FunctionParameter]
    pub fn function_parameter(
        &self,
        parameter_name: &str,
        arg_type: TypeNameOpt,
        mode: FunctionParameterMode,
    ) -> FunctionParameter {
        FunctionParameterBuilder::default()
            .name(String::from(parameter_name))
            .arg_type(arg_type)
            .mode(mode.into())
            .defexpr(NodeInner::None.into())
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::Alias]
    pub fn alias(&self, alias_name: &str, col_names: &[&str]) -> Alias {
        let mut names = Vec::new();
        for name_item in col_names {
            let name_item = self.string(name_item);
            names.push(self.as_string_node(name_item))
        }
        let names = self.as_node_list(&names);

        AliasBuilder::default()
            .aliasname(String::from(alias_name))
            .colnames(names)
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::RangeVar]
    pub fn range_var(&self, full_name: &FullName, alias: &AliasOpt) -> crate::syn::RangeVar {
        let catalog = full_name
            .catalog()
            .map(|c| c.to_string())
            .unwrap_or_default();
        let schema = full_name
            .schema()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let rel_name = full_name.local_name().to_string();
        crate::syn::RangeVarBuilder::default()
            .catalogname(catalog)
            .schemaname(schema)
            .relname(rel_name)
            .inh(false)
            .relpersistence(String::from("p"))
            .alias(alias.clone())
            .location(0)
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::RoleSpec]
    pub fn role_spec(&self, role_name: &str) -> crate::syn::RoleSpec {
        crate::syn::RoleSpecBuilder::default()
            .rolename(role_name.to_string())
            .location(0)
            .roletype(crate::syn::RoleSpecType::RolespecCstring.into())
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::AlterTableCmd] change owner variant
    pub fn alter_table_cmd_change_owner(&self, role_name: &str) -> AlterTableCmd {
        let role_spec = self.role_spec(role_name);

        AlterTableCmdBuilder::default()
            .subtype(AlterTableType::AtChangeOwner.into())
            .name(String::default())
            .num(0)
            .newowner(role_spec.into())
            .def(Node::none())
            .behavior(DropBehaviorOpt::none())
            .missing_ok(false)
            .recurse(false)
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::Constraint] primary key variant
    pub fn constraint_primary_key(&self) -> Constraint {
        ConstraintBuilder::default()
            .contype(ConstrType::ConstrPrimary.into())
            .conname(String::default())
            .deferrable(false)
            .initdeferred(false)
            .skip_validation(false)
            .initially_valid(false)
            .is_no_inherit(false)
            .raw_expr(Node::none())
            .cooked_expr(String::default())
            .generated_when(String::default())
            .inhcount(0)
            .nulls_not_distinct(false)
            .keys(NodeList::default())
            .including(NodeList::default())
            .exclusions(NodeList::default())
            .options(NodeList::default())
            .indexname(String::default())
            .indexspace(String::default())
            .reset_default_tblspc(false)
            .access_method(String::default())
            .where_clause(Node::none())
            .pktable(RangeVarOpt::none())
            .fk_attrs(NodeList::default())
            .pk_attrs(NodeList::default())
            .fk_matchtype(String::default())
            .fk_upd_action(String::default())
            .fk_del_action(String::default())
            .fk_del_set_cols(NodeList::default())
            .old_conpfeqop(NodeList::default())
            .old_pktable_oid(0)
            .location(0)
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::AlterTableCmd] add constraint variant
    pub fn alter_table_cmd_add_constraint(&self, constraint: Constraint) -> AlterTableCmd {
        AlterTableCmdBuilder::default()
            .subtype(AlterTableType::AtAddConstraint.into())
            .name(String::default())
            .num(0)
            .newowner(RoleSpecOpt::none())
            .def(NodeInner::Constraint(constraint).into())
            .behavior(DropBehaviorOpt::none())
            .missing_ok(false)
            .recurse(false)
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::Alias]
    pub fn alias_name_only(&self, alias_name: &str) -> Alias {
        AliasBuilder::default()
            .aliasname(alias_name.to_string())
            .colnames(NodeList::default())
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::RangeSubSelect]
    pub fn range_subselect(&self, alias: Alias, query: SelectStmt) -> RangeSubselect {
        let node = Node::from(NodeInner::SelectStmt(query));
        RangeSubselectBuilder::default()
            .alias(alias.into())
            .subquery(node.into())
            .lateral(false)
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::SelectStmt]
    pub fn select_stmt(
        &self,
        with_clause: WithClauseOpt,
        from_clause: NodeList,
        target_list: Vec<ResTarget>,
    ) -> SelectStmt {
        let nodes: Vec<Node> = target_list
            .into_iter()
            .map(|item| Node::from(NodeInner::ResTarget(item)))
            .collect();
        let target_list = NodeList::from(nodes);

        crate::syn::SelectStmtBuilder::default()
            .with_clause(with_clause)
            .from_clause(from_clause)
            .target_list(target_list)
            .distinct_clause(NodeList::default())
            .into_clause(IntoClauseOpt::none())
            .where_clause(Node::none())
            .group_clause(NodeList::default())
            .group_distinct(false)
            .having_clause(Node::none())
            .window_clause(NodeList::default())
            .values_lists(NodeList::default())
            .sort_clause(NodeList::default())
            .limit_offset(Node::none())
            .limit_count(Node::none())
            .limit_option(LimitOptionOpt::none())
            .locking_clause(NodeList::default())
            .op(SetOperation::SetopNone.into())
            .all(false)
            .larg(SelectStmtOpt::none())
            .rarg(SelectStmtOpt::none())
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::ResTarget]
    pub fn res_target(&self, name: &str, val: Node) -> ResTarget {
        crate::syn::ResTargetBuilder::default()
            .name(name.to_string())
            .indirection(NodeList::default())
            .val(val)
            .location(0)
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::AConst] as Integer
    pub fn const_int4(&self, value: i32) -> AConst {
        AConstInner::Ival(
            IntegerBuilder::default()
                .ival(value)
                .build()
                .unwrap()
                .into(),
        )
        .into()
    }
}
