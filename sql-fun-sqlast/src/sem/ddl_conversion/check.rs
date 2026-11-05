use crate::ast::Table;
use crate::sem::EngineFeatureValidation;
use crate::sem::SemScalarExpr;
use crate::{
    ast::{Column, ColumnConstraint, TableConstraint, TryFromWithTokens},
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, CheckColumnConstraint, CheckConstraint,
        ColumnConstraint as SemColumnConstraint, ColumnDefinition, Constraint, ConstraintNames,
        CreateTable, ScalarAnalysisContext, problem::CheckProblem,
    },
    syn::{Constraint as SynConstraint, ScanToken},
};
use ::sql_fun_core::IVec;

impl CheckColumnConstraint {
    pub(crate) fn from_ast_legacy_inline<TContext: ParseContext>(
        ast: &TableConstraint,
        mut context: TContext,
        target: &TableName,
        column: &ColumnName,
    ) -> Result<(Self, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let canonical = context
            .resolve_table_name(target)
            .unwrap_or_else(|| target.clone());
        let table = context.get_table(&canonical).cloned();
        let scope = if let Some(table) = &table {
            ScalarAnalysisContext::declared_table(table, &canonical)
        } else {
            ScalarAnalysisContext::default()
        };
        let valid = EngineFeatureValidation::table_constraint(ast, &mut context, true)?;
        let result =
            CheckConstraint::from_ast_expression(ast, &mut context, &canonical, &scope, false);
        let mut check = context.finish_analysis(result)?;
        if !valid {
            check.disallow();
        }
        Ok((Self::from_analyzed(&canonical, column, &check), context))
    }
}

impl CheckConstraint {
    pub(crate) fn from_ast_table_constraint(
        ast: &TableConstraint,
        context: &mut impl ParseContext,
        target: &TableName,
        table: Option<&CreateTable>,
        alter: bool,
    ) -> Result<Self, AnalysisError> {
        let scope = if let Some(table) = table {
            ScalarAnalysisContext::declared_table(table, target)
        } else {
            ScalarAnalysisContext::default()
        };
        let mut check = Self::from_ast_expression(ast, context, target, &scope, alter)?;
        let name = check.name().to_owned();
        if table.is_none() {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(target))?;
            check.missing_target();
        }
        if !name.is_empty() && table.is_some_and(|table| table.has_constraint_name(&name)) {
            check.reject_duplicate_name(context)?;
        }
        Ok(check)
    }

    pub(crate) fn from_ast_expression(
        ast: &TableConstraint,
        context: &mut impl ParseContext,
        target: &TableName,
        scope: &ScalarAnalysisContext<'_>,
        alter: bool,
    ) -> Result<Self, AnalysisError> {
        let ColumnConstraint::Check(syntax) = &ast.constraint else {
            return AnalysisError::raise_unexpected_input("expected CHECK");
        };
        let expression = SemScalarExpr::from_ast_in(syntax, context, scope)?;
        let mut check = Self::analyze(
            context,
            target,
            ast.name.as_deref().unwrap_or_default(),
            &expression,
        )?;
        check.retain_options(
            ast.extras.no_inherit,
            ast.extras.skip_validation,
            ast.extras.enforced,
        );
        if ast.extras.deferrable || ast.extras.initially_deferred {
            check.reject(context, CheckProblem::Deferrability)?;
        }
        check.set_validated(!alter || !ast.extras.skip_validation);
        Ok(check)
    }

    pub(crate) fn from_legacy_syntax(
        context: &mut impl ParseContext,
        target: Option<&TableName>,
        syntax: &SynConstraint,
    ) -> Result<Constraint, AnalysisError> {
        let Some(target) = target else {
            return AnalysisError::raise_unexpected_none("CHECK target table");
        };
        let canonical = context
            .resolve_table_name(target)
            .unwrap_or_else(|| target.clone());
        let table = context.get_table(&canonical).cloned();
        let common = TableConstraint::try_from_with_tokens(syntax, &IVec::<ScanToken>::default())?;
        let valid = EngineFeatureValidation::table_constraint(&common, context, false)?;
        let mut check =
            Self::from_ast_table_constraint(&common, context, &canonical, table.as_ref(), true)?;
        if !valid {
            check.disallow();
        }
        Ok(Constraint::Check(check))
    }
}

impl CreateTable {
    pub(crate) fn resolve_ast_inline_checks(
        ast: &Table,
        context: &mut impl ParseContext,
        table: &mut Self,
        names: &mut ConstraintNames,
    ) -> Result<(), AnalysisError> {
        if !ast
            .columns
            .iter()
            .any(CheckColumnConstraint::has_ast_column_checks)
        {
            return Ok(());
        }
        let target =
            TableName::from(context.creation_name(table.name().full_name(), ast.temporary)?);
        for syntax in &ast.columns {
            if !CheckColumnConstraint::has_ast_column_checks(syntax) {
                continue;
            }
            let name = ColumnName::from(syntax.name.as_str());
            let mut column = table
                .get_column_def(&name)
                .expect("declared column")
                .clone();
            ColumnDefinition::resolve_ast_inline_checks(
                syntax,
                context,
                &target,
                Some(table),
                names,
                &mut column,
            )?;
            table.replace_declared_column(&column);
        }
        Ok(())
    }

    pub(crate) fn resolve_ast_checks(
        ast: &Table,
        context: &mut impl ParseContext,
        table: &mut Self,
        names: &mut ConstraintNames,
    ) -> Result<(), AnalysisError> {
        if !ast.constraints.iter().any(TableConstraint::is_check) {
            return Ok(());
        }
        let target = context.creation_name(table.name().full_name(), ast.temporary)?;
        for definition in &ast.constraints {
            if definition.is_check() {
                let scope =
                    ScalarAnalysisContext::declared_table(table, &TableName::from(target.clone()));
                let mut check = CheckConstraint::from_ast_expression(
                    definition,
                    context,
                    &TableName::from(target.clone()),
                    &scope,
                    false,
                )?;
                if !names.reserve(check.name()) {
                    check.reject_duplicate_name(context)?;
                }
                table.record_declared_constraint(&Constraint::Check(check))?;
            }
        }
        Ok(())
    }
}

impl ConstraintNames {
    pub(crate) fn reserve_ast_column(ast: &Column, context: &impl ParseContext, names: &mut Self) {
        for definition in &ast.constraints {
            if !definition.is_check()
                && Self::ast_constraint_has_catalog_name(&definition.constraint, context)
            {
                names.reserve(definition.name.as_deref().unwrap_or_default());
            }
        }
    }
}

impl CheckColumnConstraint {
    pub(crate) fn has_ast_column_checks(ast: &Column) -> bool {
        ast.constraints.iter().any(TableConstraint::is_check)
    }
}

impl ColumnDefinition {
    pub(crate) fn resolve_ast_inline_checks(
        ast: &Column,
        context: &mut impl ParseContext,
        target: &TableName,
        table: Option<&CreateTable>,
        names: &mut ConstraintNames,
        column: &mut Self,
    ) -> Result<(), AnalysisError> {
        if !CheckColumnConstraint::has_ast_column_checks(ast) {
            return Ok(());
        }
        let scope = if let Some(table) = table {
            ScalarAnalysisContext::declared_table(table, target)
        } else {
            ScalarAnalysisContext::column_declaration(context, target, column)
        };
        let name = ColumnName::from(ast.name.as_str());
        let mut index = 0;
        for definition in &ast.constraints {
            if definition.is_check() {
                let mut check = CheckConstraint::from_ast_expression(
                    definition, context, target, &scope, false,
                )?;
                if !names.reserve(check.name()) {
                    check.reject_duplicate_name(context)?;
                }
                let check = CheckColumnConstraint::from_analyzed(target, &name, &check);
                column.insert_inline_check(index, &SemColumnConstraint::check(check));
            }
            if !matches!(definition.constraint, ColumnConstraint::Enforcement(_)) {
                index += 1;
            }
        }
        Ok(())
    }
}

impl SemColumnConstraint {
    pub(crate) fn from_ast_standalone_check(
        ast: &ColumnConstraint,
        context: &mut impl ParseContext,
        target: &TableName,
        column: &ColumnName,
    ) -> Result<Self, AnalysisError> {
        let canonical = context
            .resolve_table_name(target)
            .unwrap_or_else(|| target.clone());
        let definition = context.get_table(&canonical).cloned();
        let scope = if let Some(definition) = &definition {
            ScalarAnalysisContext::declared_table(definition, &canonical)
        } else {
            ScalarAnalysisContext::default()
        };
        let syntax = TableConstraint::unnamed_check(ast);
        let check =
            CheckConstraint::from_ast_expression(&syntax, context, &canonical, &scope, false)?;
        Ok(Self::check(CheckColumnConstraint::from_analyzed(
            &canonical, column, &check,
        )))
    }
}
