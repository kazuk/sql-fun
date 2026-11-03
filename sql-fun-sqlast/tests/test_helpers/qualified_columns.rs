pub(crate) fn assert_problem_code(
    context: &::sql_fun_sqlast::context::SchemaFileContext,
    code: &str,
) {
    for problem in context.problems() {
        if problem.code() == code {
            return;
        }
    }
    panic!("expected {code}: {:?}", context.problems());
}
