use crate::{
    ast::Name,
    names::{OperatorName, ValidOperatorName},
    sem::ProblemLikeError,
};

impl OperatorName {
    pub(crate) fn from_ast_name(ast: &Name) -> Result<Self, ProblemLikeError> {
        ValidOperatorName::try_from_components(&ast.0).map(|name| Self::from_valid(&name))
    }
}

#[cfg(test)]
mod operator_name_validation_tests {
    use crate::{ast::Name, names::OperatorName};

    #[rstest::rstest]
    #[case(&[], "operator_name_syntax")]
    #[case(&[""], "operator_name_syntax")]
    #[case(&["schema", ""], "operator_name_syntax")]
    #[case(&["a", "b", "+"], "cross_database_operator_reference")]
    #[case(&["a", "b", "c", "+"], "cross_database_operator_reference")]
    fn common_names_classify_invalid_components(#[case] parts: &[&str], #[case] code: &str) {
        let parts: Vec<_> = parts.iter().map(|part| (*part).to_owned()).collect();
        let problem = OperatorName::from_ast_name(&Name::from_parts(&parts))
            .unwrap_err()
            .into_problem();
        assert_eq!(problem.code(), code);
        assert!(problem.render_metadata().is_some());
    }
}
