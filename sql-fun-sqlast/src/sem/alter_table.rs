use sql_fun_core::IVec;

use crate::{
    sem::{
        AlterSequence, AnalizeConstraint, AstAndContextPair, ColumnName, Constraint, FromClause,
        FullName, SemScalarExpr, ViewName, WithClause, analyze_scaler_expr,
    },
    syn::{AlterTableType, ListOpt, ObjectType, Opt, ScanToken},
};

use super::{AnalysisError, AnalysisProblem, ParseContext, SemAst, create_table::TableName};

/// analyzed alter table statement
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct AlterTable {
    table: TableName,
    commands: Vec<AlterObjSubCommand>,
}

/// analyzed alter view statement
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct AlterView {
    view: ViewName,
    commands: Vec<AlterObjSubCommand>,
}

#[derive(Debug, Clone)]
pub enum AlterObjSubCommand {
    ChangeOwner(String),
    SetColumnDefault(ColumnName, Box<SemScalarExpr>),
    AddConstraint(Constraint),
    ClusterOn(String),
}

fn analyze_alter_obj_change_owner<TParseContext>(
    _context: &TParseContext,
    cmd: crate::syn::AlterTableCmd,
) -> Result<AlterObjSubCommand, AnalysisError> {
    let Some(new_owner) = cmd.get_newowner().get_rolename() else {
        AnalysisError::raise_unexpected_none("alter_table_cmd.newwoner")?
    };
    Ok(AlterObjSubCommand::ChangeOwner(new_owner))
}

#[cfg(test)]
mod test_analyze_alter_obj_change_owner {
    use crate::{
        sem::alter_table::analyze_alter_obj_change_owner,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };
    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_alter_obj_change_owner(test_context: TestParseContext) -> TestResult {
        let alter_table_cmd = SynBuilder::new().alter_table_cmd_change_owner("test");
        let _result = analyze_alter_obj_change_owner(&test_context, alter_table_cmd)?;
        Ok(())
    }
}

fn analyze_alter_obj_add_constraint<TParseContext>(
    context: &mut TParseContext,
    target_table: &TableName,
    cmd: crate::syn::AlterTableCmd,
) -> Result<AlterObjSubCommand, AnalysisError>
where
    TParseContext: ParseContext,
{
    let _constraint_name = cmd.get_name();
    let Some(constraint) = cmd.get_def().as_constraint().as_inner() else {
        AnalysisError::raise_unexpected_none("alter_table_cmd.def as constraint")?
    };
    let constraint = Constraint::analyze(context, Some(target_table), constraint)?;
    Ok(AlterObjSubCommand::AddConstraint(constraint))
}

#[cfg(test)]
mod test_analyze_alter_obj_add_constraint {
    use testresult::TestResult;

    use crate::{
        sem::{CreateTable, FullName, TableName},
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    #[rstest::rstest]
    fn test_analyze_alter_obj_add_constraint(mut test_context: TestParseContext) -> TestResult {
        let builder = SynBuilder::new();
        let constraint = builder.constraint_primary_key();
        let alter_table_cmd = builder.alter_table_cmd_add_constraint(constraint);
        let table_name = TableName::from(FullName::with_schema("public", "table1"));
        let create_table = CreateTable::new(table_name.clone(), vec![]);
        test_context.set_get_table_impl_result(table_name.full_name(), &Some(create_table));
        let _result = super::analyze_alter_obj_add_constraint(
            &mut test_context,
            &table_name,
            alter_table_cmd,
        )?;
        Ok(())
    }
}

fn analyze_alter_obj_column_default<TParseContext>(
    mut context: TParseContext,
    cmd: crate::syn::AlterTableCmd,
    tokens: &IVec<ScanToken>,
) -> Result<(AlterObjSubCommand, TParseContext), AnalysisError>
where
    TParseContext: ParseContext,
{
    let column_name = ColumnName::from(cmd.get_name().clone());
    let with_clause = WithClause::default();
    let from_clause = FromClause::default();
    let (expr, new_context) =
        analyze_scaler_expr(context, &with_clause, &from_clause, cmd.get_def(), tokens)?;
    context = new_context;

    // FIXME: check table, column existing

    Ok((
        AlterObjSubCommand::SetColumnDefault(column_name, Box::new(expr)),
        context,
    ))
}

#[cfg(test)]
mod test_analyze_alter_obj_column_default {
    use crate::{
        sem::{ScalarConstExpr, SemScalarExpr, alter_table::AlterObjSubCommand},
        syn::NodeInner,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };
    use sql_fun_core::IVec;
    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_alter_obj_column_default(test_context: TestParseContext) -> TestResult {
        // alter table ...
        //   alter column value set default(0)
        let builder = SynBuilder::new();
        let value = builder.const_int4(0);
        let cmd =
            builder.alter_table_cmd_set_column_default("value", NodeInner::AConst(value).into());
        let tokens = IVec::default();

        let (res, _) = super::analyze_alter_obj_column_default(test_context, cmd, &tokens)?;
        let AlterObjSubCommand::SetColumnDefault(col_name, expr) = res else {
            panic!("unexpected respose {res:#?}");
        };
        assert_eq!(col_name.as_str(), "value");
        let SemScalarExpr::Const(ScalarConstExpr::Integer(val)) = *expr else {
            panic!("unexpected expr {expr:#?}");
        };
        assert_eq!(val, 0);

        Ok(())
    }
}

pub fn analyze_alter_table_cmd<TParseContext>(
    mut context: TParseContext,
    table_name: &TableName,
    cmd: crate::syn::AlterTableCmd,
    subtyp: AlterTableType,
    tokens: &IVec<ScanToken>,
) -> Result<(AlterObjSubCommand, TParseContext), AnalysisError>
where
    TParseContext: ParseContext,
{
    let sub_cmd = match subtyp {
        AlterTableType::AtChangeOwner => analyze_alter_obj_change_owner(&context, cmd)?,
        AlterTableType::AtColumnDefault => {
            let (column_default, new_context) =
                analyze_alter_obj_column_default(context, cmd, tokens)?;
            context = new_context;
            column_default
        }
        AlterTableType::AtAddConstraint => {
            analyze_alter_obj_add_constraint(&mut context, table_name, cmd)?
        }
        AlterTableType::AtClusterOn => {
            let index_name = cmd.get_name();
            if context.get_index(&index_name).is_none() {
                context.report_problem(AnalysisProblem::index_not_found(&index_name))?;
            }
            AlterObjSubCommand::ClusterOn(cmd.get_name())
        }

        _ => todo!("{subtyp:?}"),
    };
    Ok((sub_cmd, context))
}

pub fn analyze_alter_table_statement<TParseContext>(
    mut context: TParseContext,
    _parent_schema: &Option<String>,
    syn: crate::syn::AlterTableStmt,
    tokens: &IVec<ScanToken>,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    let Some(rel) = syn.get_relation().as_inner() else {
        AnalysisError::raise_unexpected_none("alter_table_stmt.relation")?
    };
    let table_name = TableName::try_from(rel)?;
    if context.get_table(&table_name).is_none() {
        context.report_problem(AnalysisProblem::relation_not_found(&table_name))?;
    }

    let Some(cmds) = syn.get_cmds().map(|c| c.as_alter_table_cmd()) else {
        AnalysisError::raise_unexpected_none("alter_table_stmt.cmds")?
    };
    let mut sub_cmds = Vec::new();
    for cmd in cmds {
        let Some(subtyp) = cmd.get_subtype().as_inner() else {
            AnalysisError::raise_unexpected_none("alter_table_cmd.subtype")?
        };
        let (sub_cmd, new_context) =
            analyze_alter_table_cmd(context, &table_name, cmd, subtyp, tokens)?;
        context = new_context;
        sub_cmds.push(sub_cmd);
    }
    let alter_table = AlterTable {
        table: table_name,
        commands: sub_cmds,
    };
    context = context.apply_alter_table(&alter_table)?;
    Ok(AstAndContextPair::new(
        SemAst::AlterTable(alter_table),
        context,
    ))
}

#[cfg(test)]
mod test_analyze_alter_table_statement {
    use crate::{
        AstAndContextPair,
        sem::{FullName, SemAst},
        syn::{AliasOpt, NodeInner, ObjectType},
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };
    use sql_fun_core::IVec;
    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_alter_table_statement(mut test_context: TestParseContext) -> TestResult {
        test_context.setup_table("table1", &[]);

        let builder = SynBuilder::new();
        let relation = builder.range_var(
            &FullName::with_schema("public", "table1"),
            &AliasOpt::none(),
        );
        let cmd = builder.alter_table_cmd_set_column_default(
            "col1",
            NodeInner::AConst(builder.const_int4(10)).into(),
        );
        let cmds = builder.as_node_list(&[NodeInner::AlterTableCmd(cmd).into()]);
        let alter_table_stmt =
            builder.alter_table_stmt(relation.into(), cmds, ObjectType::ObjectTable.into());
        let tokens = IVec::default();
        let AstAndContextPair(res, ctx) =
            super::analyze_alter_table_statement(test_context, &None, alter_table_stmt, &tokens)?;
        let SemAst::AlterTable(at) = res else {
            panic!("unexpected type {res:?}")
        };
        assert_eq!(at.commands.len(), 1);
        assert_eq!(ctx.alter_tables_len(), 1);
        Ok(())
    }
}

#[tracing::instrument(skip(context, _parent_schema, syn))]
pub fn analyze_alter_view_statement<TParseContext>(
    mut context: TParseContext,
    _parent_schema: &Option<String>,
    as_matview: bool,
    syn: crate::syn::AlterTableStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    let Some(rel) = syn.get_relation().as_inner() else {
        AnalysisError::raise_unexpected_none("alter_table_stmt.relation")?
    };
    let view_name = ViewName::from(rel);
    if let Some(view) = context.get_view(&view_name) {
        if as_matview && !view.is_materialized() {
            context.report_problem(AnalysisProblem::view_not_matview(&view_name))?;
        }
    } else {
        context.report_problem(AnalysisProblem::view_not_found(&view_name))?;
    }

    let Some(cmds) = syn.get_cmds().map(|c| c.as_alter_table_cmd()) else {
        AnalysisError::raise_unexpected_none("alter_table_stmt.cmds")?
    };
    let mut sub_cmds = Vec::new();
    for cmd in cmds {
        let Some(subtyp) = cmd.get_subtype().as_inner() else {
            AnalysisError::raise_unexpected_none("alter_table_cmd.subtype")?
        };
        let sub_cmd = match subtyp {
            AlterTableType::AtChangeOwner => analyze_alter_obj_change_owner(&context, cmd)?,

            _ => todo!("{subtyp:?}"),
        };
        sub_cmds.push(sub_cmd);
    }
    let alter_view = AlterView {
        view: view_name,
        commands: sub_cmds,
    };

    context = context.apply_alter_view(&alter_view)?;
    Ok(AstAndContextPair::new(
        SemAst::AlterView(alter_view),
        context,
    ))
}

pub fn analyze_alter_seq_statement<TParseContext>(
    mut context: TParseContext,
    _parent_schema: &Option<String>,
    syn: crate::syn::AlterTableStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    let Some(rel) = syn.get_relation().as_inner() else {
        AnalysisError::raise_unexpected_none("alter_table_stmt.relation")?
    };
    let seq_name = FullName::try_from(rel)?;
    if context.get_sequence(&seq_name).is_none() {
        context.report_problem(AnalysisProblem::sequence_not_found(&seq_name))?;
    }
    let Some(cmds) = syn.get_cmds().map(|c| c.as_alter_table_cmd()) else {
        AnalysisError::raise_unexpected_none("alter_table_stmt.cmds")?
    };
    let mut sub_cmds = Vec::new();

    for cmd in cmds {
        let Some(subtyp) = cmd.get_subtype().as_inner() else {
            AnalysisError::raise_unexpected_none("alter_table_cmd.subtype")?
        };
        let sub_cmd = match subtyp {
            AlterTableType::AtChangeOwner => analyze_alter_obj_change_owner(&context, cmd)?,

            _ => todo!("{subtyp:?}"),
        };
        sub_cmds.push(sub_cmd);
    }

    let alter_seq = AlterSequence::new(&seq_name, &sub_cmds);

    context = context.apply_alter_sequence(&alter_seq)?;
    Ok(AstAndContextPair::new(
        SemAst::AlterSequence(alter_seq),
        context,
    ))
}

/// analyze [`crate::syn::AlterTableStmt`]
#[tracing::instrument(skip(context, parent_schema, syn))]
pub fn analyze_alter_statement<TParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    syn: crate::syn::AlterTableStmt,
    tokens: &IVec<ScanToken>,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    let Some(obj_type) = syn.get_objtype().as_inner() else {
        AnalysisError::raise_unexpected_none("alter_table_stmt.objtype")?
    };
    match obj_type {
        ObjectType::ObjectTable => {
            analyze_alter_table_statement(context, parent_schema, syn, tokens)
        }
        ObjectType::ObjectView => analyze_alter_view_statement(context, parent_schema, false, syn),
        ObjectType::ObjectSequence => analyze_alter_seq_statement(context, parent_schema, syn),
        ObjectType::ObjectMatview => {
            analyze_alter_view_statement(context, parent_schema, true, syn)
        }
        _ => todo!("{obj_type:?}"),
    }
}
