use crate::{
    sem::FullName,
    syn::{
        AConst, AConstInner, AExprKind, Alias, AliasBuilder, AliasOpt, AlterTableCmd,
        AlterTableCmdBuilder, AlterTableStmt, AlterTableStmtBuilder, AlterTableType,
        BooleanBuilder, CaseExpr, CaseWhen, CoercionFormOpt, ConstrType, Constraint,
        ConstraintBuilder, DropBehaviorOpt, FuncCallBuilder, FunctionParameter,
        FunctionParameterBuilder, FunctionParameterMode, IndexElemBuilder, IndexStmtBuilder,
        IntegerBuilder, IntoClauseOpt, JoinExpr, JoinTypeOpt, LimitOptionOpt, ListBuilder, Node,
        NodeInner, NodeList, NodeOpt, ObjectType, RangeFunction, RangeFunctionBuilder,
        RangeSubselect, RangeSubselectBuilder, RangeTableFunc, RangeTableFuncBuilder, RangeVarOpt,
        ResTarget, RoleSpecOpt, SelectStmt, SelectStmtOpt, SetOperation, SortByDirOpt,
        SortByNullsOpt, StringBuilder, TypeName, TypeNameOpt, WindowDefOpt, WithClauseOpt,
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

    /// create [crate::syn::FuncCall] with common defaults
    pub fn func_call(&self, funcname: NodeList, args: NodeList) -> crate::syn::FuncCall {
        FuncCallBuilder::default()
            .funcname(funcname)
            .args(args)
            .agg_order(NodeList::default())
            .agg_filter(Node::none())
            .over(WindowDefOpt::none())
            .agg_within_group(false)
            .agg_star(false)
            .agg_distinct(false)
            .func_variadic(false)
            .funcformat(CoercionFormOpt::none())
            .location(1)
            .build()
            .unwrap()
            .into()
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

    /// create [crate::syn::AlterTableCmd] set column default
    pub fn alter_table_cmd_set_column_default(
        &self,
        column_name: &str,
        default_expr: Node,
    ) -> AlterTableCmd {
        AlterTableCmdBuilder::default()
            .subtype(AlterTableType::AtColumnDefault.into())
            .name(column_name.to_string())
            .num(0)
            .newowner(RoleSpecOpt::none())
            .def(default_expr)
            .behavior(DropBehaviorOpt::none())
            .missing_ok(false)
            .recurse(false)
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::AlterTableStmt]
    pub fn alter_table_stmt(
        &self,
        relation: RangeVarOpt,
        cmds: NodeList,
        objtype: ObjectType,
    ) -> AlterTableStmt {
        AlterTableStmtBuilder::default()
            .relation(relation)
            .cmds(cmds)
            .objtype(objtype.into())
            .missing_ok(false)
            .build()
            .unwrap()
            .into()
    }

    fn constraint_builder(contype: crate::syn::ConstrTypeOpt) -> ConstraintBuilder {
        let mut builder = ConstraintBuilder::default();
        builder
            .contype(contype)
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
            .location(0);
        builder
    }

    /// create [crate::syn::Constraint] primary key variant
    pub fn constraint_primary_key(&self) -> Constraint {
        Self::constraint_builder(ConstrType::ConstrPrimary.into())
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::Constraint] not null variant
    pub fn constraint_not_null(&self) -> Constraint {
        Self::constraint_builder(ConstrType::ConstrNotnull.into())
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::Constraint] null variant
    pub fn constraint_null(&self) -> Constraint {
        Self::constraint_builder(ConstrType::ConstrNull.into())
            .build()
            .unwrap()
            .into()
    }

    /// crate [crate::syn::Constraint] default variant
    pub fn constraint_default(&self) -> Constraint {
        let expr = NodeInner::AConst(self.const_int4(0)).into();
        Self::constraint_builder(ConstrType::ConstrDefault.into())
            .raw_expr(expr)
            .build()
            .unwrap()
            .into()
    }

    /// crate [crate::syn::Constraint] identity variant
    pub fn constraint_identity(&self, generated_when: &str) -> Constraint {
        Self::constraint_builder(ConstrType::ConstrIdentity.into())
            .generated_when(String::from(generated_when))
            .build()
            .unwrap()
            .into()
    }

    /// crate [crate::syn::Constraint] generated variant
    pub fn constraint_generated(&self, generated_when: &str) -> Constraint {
        let expr = NodeInner::AConst(self.const_int4(0)).into();
        Self::constraint_builder(ConstrType::ConstrGenerated.into())
            .generated_when(String::from(generated_when))
            .raw_expr(expr)
            .build()
            .unwrap()
            .into()
    }

    /// crate [crate::syn::Constraint] unique variant
    pub fn constraint_unique(&self) -> Constraint {
        Self::constraint_builder(ConstrType::ConstrUnique.into())
            .build()
            .unwrap()
            .into()
    }

    /// crate [crate::syn::Constraint] identity variant
    pub fn constraint_check(&self) -> Constraint {
        let expr = NodeInner::AConst(
            AConstInner::Boolval(
                BooleanBuilder::default()
                    .boolval(false)
                    .build()
                    .unwrap()
                    .into(),
            )
            .into(),
        )
        .into();
        Self::constraint_builder(ConstrType::ConstrCheck.into())
            .raw_expr(expr)
            .build()
            .unwrap()
            .into()
    }

    /// crate [crate::syn::Constraint] exclusion variant
    pub fn constraint_exclusion(&self) -> Constraint {
        Self::constraint_builder(ConstrType::ConstrExclusion.into())
            .build()
            .unwrap()
            .into()
    }

    /// crate [crate::syn::Constraint] foreign variant
    pub fn constraint_foreign(&self) -> Constraint {
        Self::constraint_builder(ConstrType::ConstrForeign.into())
            .pktable(
                self.range_var(&FullName::from_local_name("pktable"), &AliasOpt::none())
                    .into(),
            )
            .pk_attrs(
                self.as_node_list(&[self.as_string_node(self.string("pkcol"))])
                    .into(),
            )
            .build()
            .unwrap()
            .into()
    }

    /// crate [crate::syn::Constraint] attr deferrable variant
    pub fn constraint_attr_deferrable(&self) -> Constraint {
        Self::constraint_builder(ConstrType::ConstrAttrDeferrable.into())
            .build()
            .unwrap()
            .into()
    }

    /// crate [crate::syn::Constraint] attr not deferrable variant
    pub fn constraint_attr_not_deferrable(&self) -> Constraint {
        Self::constraint_builder(ConstrType::ConstrAttrNotDeferrable.into())
            .build()
            .unwrap()
            .into()
    }

    /// crate [crate::syn::Constraint] attr deferred variant
    pub fn constraint_attr_deferred(&self) -> Constraint {
        Self::constraint_builder(ConstrType::ConstrAttrDeferred.into())
            .build()
            .unwrap()
            .into()
    }

    /// crate [crate::syn::Constraint] attr immediate variant
    pub fn constraint_attr_immediate(&self) -> Constraint {
        Self::constraint_builder(ConstrType::ConstrAttrImmediate.into())
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

    /// create [crate::syn::IndexElem] for column
    pub fn index_elem_column(
        &self,
        name: &str,
        ordering: SortByDirOpt,
        index_col_name: Option<&str>,
    ) -> crate::syn::IndexElem {
        IndexElemBuilder::default()
            .name(name.to_string())
            .expr(Node::none())
            .indexcolname(index_col_name.unwrap_or_default().to_string())
            .collation(NodeList::default())
            .opclass(NodeList::default())
            .opclassopts(NodeList::default())
            .ordering(ordering)
            .nulls_ordering(SortByNullsOpt::none())
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::IndexElem] for expression
    pub fn index_elem_expr(
        &self,
        expr: Node,
        ordering: SortByDirOpt,
        index_col_name: Option<&str>,
    ) -> crate::syn::IndexElem {
        IndexElemBuilder::default()
            .name(String::default())
            .expr(expr)
            .indexcolname(index_col_name.unwrap_or_default().to_string())
            .collation(NodeList::default())
            .opclass(NodeList::default())
            .opclassopts(NodeList::default())
            .ordering(ordering)
            .nulls_ordering(SortByNullsOpt::none())
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::NodeList] of [crate::syn::IndexElem]
    pub fn index_elem_list(&self, items: Vec<crate::syn::IndexElem>) -> NodeList {
        let nodes: Vec<Node> = items
            .into_iter()
            .map(|item| Node::from(NodeInner::IndexElem(item)))
            .collect();
        NodeList::from(nodes)
    }

    /// create [crate::syn::IndexStmt]
    pub fn index_stmt(
        &self,
        idxname: &str,
        relation: RangeVarOpt,
        index_params: NodeList,
        index_including_params: NodeList,
        unique: bool,
    ) -> crate::syn::IndexStmt {
        IndexStmtBuilder::default()
            .idxname(idxname.to_string())
            .relation(relation)
            .access_method(String::default())
            .table_space(String::default())
            .index_params(index_params)
            .index_including_params(index_including_params)
            .options(NodeList::default())
            .where_clause(Node::none())
            .exclude_op_names(NodeList::default())
            .idxcomment(String::default())
            .index_oid(0)
            .old_number(0)
            .old_create_subid(0)
            .old_first_relfilelocator_subid(0)
            .unique(unique)
            .nulls_not_distinct(false)
            .primary(false)
            .isconstraint(false)
            .deferrable(false)
            .initdeferred(false)
            .transformed(false)
            .concurrent(false)
            .if_not_exists(false)
            .reset_default_tblspc(false)
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

    /// create [crate::syn::AConst] as Sval
    pub fn const_sval(&self, value: &str) -> AConst {
        AConstInner::Sval(
            StringBuilder::default()
                .sval(String::from(value))
                .build()
                .unwrap()
                .into(),
        )
        .into()
    }

    /// create [crate::syn::CaseExpr]
    pub fn case_expr(&self, arg: NodeOpt, args: NodeList, default: NodeOpt) -> CaseExpr {
        crate::syn::CaseExprBuilder::default()
            .xpr(Node::none())
            .casetype(0)
            .casecollid(0)
            .arg(arg)
            .args(args)
            .defresult(default)
            .location(0)
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::CaseWhen]
    pub fn case_when(&self, expr: NodeOpt, result: NodeOpt) -> CaseWhen {
        crate::syn::CaseWhenBuilder::default()
            .xpr(Node::none())
            .expr(expr)
            .result(result)
            .location(0)
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::JoinExpr]
    pub fn join_expr(
        &self,
        join_type: JoinTypeOpt,
        left: NodeOpt,
        right: NodeOpt,
        qualifications: NodeOpt,
        alias: AliasOpt,
        using_clause: &[&str],
        join_using_alias: AliasOpt,
        is_natural: bool,
    ) -> JoinExpr {
        let using_items = self.string_node_list(using_clause);
        crate::syn::JoinExprBuilder::default()
            .jointype(join_type)
            .is_natural(is_natural)
            .larg(left)
            .rarg(right)
            .quals(qualifications)
            .alias(alias)
            .using_clause(using_items)
            .join_using_alias(join_using_alias)
            .rtindex(0)
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::AExpr]
    pub fn binary(&self, operator: &str, left: Node, right: Node) -> crate::syn::AExpr {
        let name = self.as_node_list(&[self.as_string_node(self.string(operator))]);

        crate::syn::AExprBuilder::default()
            .kind(AExprKind::AexprOp.into())
            .name(name)
            .lexpr(left)
            .rexpr(right)
            .location(0)
            .build()
            .unwrap()
            .into()
    }

    /// create equals [crate::syn::AExpr] as [crate::syn::Node]
    pub fn operator_equals(&self, left: Node, right: Node) -> crate::syn::Node {
        NodeInner::AExpr(self.binary("=", left, right)).into()
    }

    /// create a [crate::syn::NodeList] of [crate::syn::String]
    pub fn string_node_list(&self, items: &[&str]) -> crate::syn::NodeList {
        let mut nodes = Vec::new();
        for item in items {
            let item = self.string(&item);
            nodes.push(self.as_string_node(item))
        }
        self.as_node_list(&nodes)
    }

    /// create [crate::syn::ColumnRef]
    pub fn column_ref(&self, rel_name: &str, col_name: &str) -> crate::syn::ColumnRef {
        let fields = self.string_node_list(&vec![rel_name, col_name]);
        crate::syn::ColumnRefBuilder::default()
            .fields(fields)
            .location(0)
            .build()
            .unwrap()
            .into()
    }

    /// create [crate::syn::FuncCall] as Node
    pub fn build_func_call_node(&self, func_name: &str) -> Node {
        let func_name_node = self.as_string_node(self.string(func_name));
        let func_name = NodeList::from(vec![func_name_node]);
        let func_call = FuncCallBuilder::default()
            .funcname(func_name)
            .args(NodeList::default())
            .agg_order(NodeList::default())
            .agg_filter(Node::none())
            .over(WindowDefOpt::none())
            .agg_within_group(false)
            .agg_star(false)
            .agg_distinct(false)
            .func_variadic(false)
            .funcformat(CoercionFormOpt::none())
            .location(0)
            .build()
            .unwrap();
        NodeInner::FuncCall(func_call.into()).into()
    }

    /// build [crate::syn::RangeFunction]
    pub fn build_range_function(
        &self,
        func_call_node: Node,
        alias_name: &str,
        is_rows_from: bool,
    ) -> RangeFunction {
        RangeFunctionBuilder::default()
            .lateral(false)
            .ordinality(false)
            .is_rowsfrom(is_rows_from)
            .functions(NodeList::from(vec![func_call_node]))
            .alias(AliasOpt::from(self.alias(alias_name, &[])))
            .coldeflist(NodeList::default())
            .build()
            .unwrap()
            .into()
    }

    /// build [crate::syn::List] as Node
    pub fn build_list_node(&self, nodes: Vec<Node>) -> Node {
        let list = ListBuilder::default()
            .items(NodeList::from(nodes))
            .build()
            .unwrap();
        NodeInner::List(list.into()).into()
    }

    /// build [crate::syn::RangeTableFunc]
    pub fn build_range_table_func(&self, func_call_node: Node, alias: Alias) -> RangeTableFunc {
        RangeTableFuncBuilder::default()
            .lateral(false)
            .docexpr(Node::none())
            .rowexpr(func_call_node)
            .namespaces(NodeList::default())
            .columns(NodeList::default())
            .alias(AliasOpt::from(alias))
            .location(0)
            .build()
            .unwrap()
            .into()
    }
}
