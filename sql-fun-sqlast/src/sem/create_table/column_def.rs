use sql_fun_core::IVec;

use crate::{
    sem::{
        AnalysisError, AnalysisProblem, ColumnConstraint, ColumnName, FullName, ParseContext,
        SemScalarExpr, TableName, TypeReference,
    },
    syn::{ListOpt, Opt, ScanToken},
};

/// column definitions
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColumnDefinition {
    name: Option<ColumnName>,
    type_ref: Option<TypeReference>,
    is_not_null: Option<bool>,
    default: Option<Box<SemScalarExpr>>,
    constraints: Vec<ColumnConstraint>,
}

impl Clone for ColumnDefinition {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            type_ref: self.type_ref.clone(),
            is_not_null: self.is_not_null,
            default: Default::default(),
            constraints: self.constraints.clone(),
        }
    }
}

impl ColumnDefinition {
    /// create instance
    #[must_use]
    pub fn new(
        name: &Option<ColumnName>,
        type_ref: Option<&TypeReference>,
        is_not_null: Option<bool>,
        constraints: &[ColumnConstraint],
    ) -> Self {
        Self {
            name: name.clone(),
            type_ref: type_ref.cloned(),
            is_not_null,
            default: Default::default(),
            constraints: constraints.to_vec(),
        }
    }

    /// get name of column
    #[must_use]
    pub fn name(&self) -> &Option<ColumnName> {
        &self.name
    }

    /// get [`TypeReference`]
    #[must_use]
    pub fn get_type_cloned(&self) -> Option<TypeReference> {
        self.type_ref.clone()
    }

    /// get [`TypeReference`]
    #[must_use]
    pub fn get_type(&self) -> Option<&TypeReference> {
        self.type_ref.as_ref()
    }

    /// column nullability
    #[must_use]
    pub fn is_not_null(&self) -> Option<bool> {
        self.is_not_null
    }

    /// apply default expression to column
    pub fn set_default(&mut self, scalar_expr: &SemScalarExpr) {
        self.default = Some(Box::new(scalar_expr.clone()));
    }

    /// apply not null constraint to column
    pub fn set_not_null(&mut self, not_null: Option<bool>) {
        self.is_not_null = not_null;
    }

    /// get column constraints
    #[must_use]
    pub fn constraints(&self) -> &[ColumnConstraint] {
        self.constraints.as_slice()
    }

    /// add column constraint
    pub fn push_constraint(&mut self, constraint: ColumnConstraint) {
        self.constraints.push(constraint)
    }

    /// analyze [`crate::syn::ColumnDef`]
    pub fn analyze_column_def<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        column_def: &crate::syn::ColumnDef,
        tokens: &IVec<ScanToken>,
    ) -> Result<(Self, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let colname = column_def.get_colname();
        let colname = ColumnName::new(&colname);
        let Some(type_name) = column_def.get_type_name().as_inner() else {
            AnalysisError::raise_unexpected_none("column_def.get_type_name")?
        };
        let is_not_null = column_def.get_is_not_null();

        let key = FullName::try_from(type_name)?;
        let type_ref = if let Some(sem_type) = context.get_type(&key).cloned() {
            if let Some(type_ref) = sem_type.type_reference().cloned() {
                Some(type_ref)
            } else {
                context.report_problem(AnalysisProblem::unexpected_dynamic_type(&sem_type))?;
                None
            }
        } else {
            context.report_problem(AnalysisProblem::column_type_not_found(
                table_name, &colname, &key,
            ))?;
            None
        };

        let constraints = if let Some(constraint_list) =
            column_def.get_constraints().map(|v| v.as_constraint())
        {
            let mut constraints = Vec::new();
            for constraint in constraint_list {
                let (c, new_context) = ColumnConstraint::analyze_constraint(
                    context,
                    table_name,
                    &colname,
                    &constraint,
                    tokens,
                )?;
                context = new_context;

                if let Some(fk) = c.as_foreign_key()
                    && let Some(col_type) = &type_ref
                    && let Some(pk_col_type) = fk.primary_key_column_type()
                    && col_type != pk_col_type
                {
                    context.report_problem(AnalysisProblem::foreign_key_column_type_mismatch(
                        table_name,
                        &colname,
                        col_type,
                        fk.primary_key_table(),
                        fk.primary_key_column(),
                        pk_col_type,
                    ))?;
                }

                constraints.push(c)
            }
            constraints
        } else {
            Vec::default()
        };
        let column = Self::new(
            &Some(colname),
            type_ref.as_ref(),
            Some(is_not_null),
            &constraints,
        );
        Ok((column, context))
    }
}
