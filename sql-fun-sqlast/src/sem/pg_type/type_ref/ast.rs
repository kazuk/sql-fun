use crate::{
    ast::TypeName,
    names::FullName,
    sem::{AnalysisError, TypeReference},
};

impl TypeReference {
    pub(crate) fn from_ast(ast: &TypeName) -> Result<Self, AnalysisError> {
        Ok(Self::concrete_type_ref(
            &FullName::from_ast_name(&ast.name)?,
            !ast.array_bounds.is_empty(),
        ))
    }
}

impl TypeReference {
    pub(crate) fn rule_shape_needs_modifiers(&self) -> bool {
        if self.is_dynamic() {
            return false;
        }
        let Some(name) = self.full_name() else {
            return false;
        };
        name.schema()
            .is_some_and(|schema| schema.as_str() == "pg_catalog")
            && matches!(
                name.name().as_str(),
                "varchar"
                    | "bpchar"
                    | "numeric"
                    | "time"
                    | "timetz"
                    | "timestamp"
                    | "timestamptz"
                    | "interval"
                    | "bit"
                    | "varbit"
            )
    }
}
