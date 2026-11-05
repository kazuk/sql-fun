use crate::{
    ast::{Call, Cast, Expr, Name, SqlValue, TypedColumn},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, ArithExpr, SemScalarExpr, TableFunctionOutput, TypeReference, WithTypmod,
    },
};

impl TableFunctionOutput {
    pub(crate) fn from_ast_typed_columns(
        columns: &[TypedColumn],
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut output = Self::default();
        for column in columns {
            let collation = column
                .collation
                .as_ref()
                .map(FullName::from_ast_name)
                .transpose()?;
            output.declare(
                context,
                &column.name,
                &TypeReference::from_ast(&column.type_name)?,
                &WithTypmod::<TypeReference>::ast_modifier_arguments(&column.type_name)?,
                collation.as_ref(),
            )?;
        }
        Ok(output)
    }
}

impl TableFunctionOutput {
    pub(crate) fn ast_expression_name(
        ast: &Expr,
        value: &SemScalarExpr,
    ) -> Result<String, AnalysisError> {
        match ast {
            Expr::Call(call) => Self::ast_call_name(call.as_ref()),
            Expr::SqlValue(value) => Ok(Self::ast_sql_value_name(value)),
            Expr::Column(name) => Self::ast_relation_name(name),
            Expr::Operator(operator) if ArithExpr::ast_is_null_if(&operator.kind) => {
                Ok(Self::nullif_output_name())
            }
            Expr::Cast(cast) => Self::ast_cast_name(cast.as_ref(), value),
            _ => Ok(value.table_output_name()),
        }
    }
}

impl TableFunctionOutput {
    pub(crate) fn ast_cast_name(
        ast: &Cast,
        value: &SemScalarExpr,
    ) -> Result<String, AnalysisError> {
        let name = Self::ast_expression_name(
            &ast.argument,
            value.explicit_cast_operand().unwrap_or(value),
        )?;
        if name.is_empty() {
            Ok(TypeReference::from_ast(&ast.target)?
                .full_name()
                .expect("SQL type syntax constructs a concrete reference")
                .local_name())
        } else {
            Ok(name)
        }
    }
}

impl TableFunctionOutput {
    pub(crate) fn ast_call_name(ast: &Call) -> Result<String, AnalysisError> {
        Self::ast_relation_name(&ast.name)
    }
}

impl TableFunctionOutput {
    pub(crate) fn ast_relation_name(ast: &Name) -> Result<String, AnalysisError> {
        Ok(FullName::from_ast_name(ast)?.local_name())
    }
}

impl TableFunctionOutput {
    pub(crate) fn ast_sql_value_name(ast: &SqlValue) -> String {
        ast.name.clone()
    }
}

impl TableFunctionOutput {
    pub(crate) fn nullif_output_name() -> String {
        "nullif".to_owned()
    }
}
