use crate::sem::SemAst;

use crate::names::FullName;
use crate::sem::traversal::SemanticVisitorMut;
use crate::sem::{AnalysisError, ResultColumnCollection, SemScalarExpr};

pub(crate) fn result_columns(ast: &SemAst) -> ResultColumnCollection {
    if let Some(statement) = ast.as_update() {
        statement.result_columns().clone()
    } else if let Some(statement) = ast.as_delete() {
        statement.result_columns().clone()
    } else {
        ast.clone()
            .into_select_statemtnt()
            .unwrap()
            .result_columns()
            .clone()
    }
}

#[derive(Default)]
pub(crate) struct TableBindings {
    relations: Vec<FullName>,
}
impl TableBindings {
    pub(crate) fn assert_only_relation(&self, relation: &FullName) {
        assert_eq!(self.relations.as_slice(), std::slice::from_ref(relation));
    }
}
impl SemanticVisitorMut for TableBindings {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if let Some(binding) = expression.table_column_reference() {
            self.relations.push(binding.relation().clone());
        }
        Ok(())
    }
}
#[derive(Default)]
pub(crate) struct SourceFailures {
    codes: Vec<&'static str>,
}
impl SourceFailures {
    pub(crate) fn assert_source_ambiguity(&self) {
        assert_eq!(self.codes, ["column_source_ambiguous"]);
    }
}
impl SemanticVisitorMut for SourceFailures {
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if let Some(failure) = expression.qualification_failure() {
            self.codes.push(failure.code());
        }
        Ok(())
    }
}
