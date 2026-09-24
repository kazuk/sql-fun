use crate::syn::sqlite::{Expr, QualifiedName};
use crate::{engine_specific::TursoMvccGcFeature, sem::AnalysisError};

/// pragma statement
#[derive(derive_getters::Getters, Debug)]
pub struct PragmaStatement {
    #[getter(skip)]
    name: QualifiedName,
    body: Option<PragmaBody>,
}

impl PragmaStatement {
    /// create instance
    pub const fn new(name: QualifiedName, body: Option<PragmaBody>) -> Self {
        Self { name, body }
    }

    /// Pragma name, notifying the Turso hook for the MVCC GC threshold pragma.
    pub fn name<Gate: TursoMvccGcFeature>(
        &self,
        gate: &mut Gate,
    ) -> Result<&QualifiedName, AnalysisError> {
        if self
            .name
            .name()
            .value()
            .eq_ignore_ascii_case("mvcc_gc_threshold")
        {
            gate.mvcc_gc_threshold_used(self)?;
        }
        Ok(&self.name)
    }
}

use turso_parser::ast::PragmaBody as PragmaBodyIn;

/// pragma statement body
#[derive(Debug)]
pub enum PragmaBody {
    /// equals
    Equals(Box<Expr>),
    /// call
    Call(Box<Expr>),
}

impl From<PragmaBodyIn> for PragmaBody {
    fn from(value: PragmaBodyIn) -> Self {
        match value {
            PragmaBodyIn::Equals(expr) => Self::Equals(Box::new(Expr::from(*expr))),
            PragmaBodyIn::Call(expr) => Self::Call(Box::new(Expr::from(*expr))),
        }
    }
}

#[cfg(test)]
mod test_new_pragmas {
    use super::{AnalysisError, PragmaBody, PragmaStatement, TursoMvccGcFeature};
    use crate::syn::sqlite::{Expr, Literal, Stmt};
    use crate::test_helpers::SqliteSynBuilder;

    #[derive(Default)]
    struct Gate(usize);
    impl TursoMvccGcFeature for Gate {
        fn mvcc_gc_threshold_used(&mut self, _: &PragmaStatement) -> Result<(), AnalysisError> {
            self.0 += 1;
            Ok(())
        }
    }

    #[test]
    fn generic_pragma_preserves_names_and_bodies() {
        for name in ["mvcc_gc_threshold", "MVCC_GC_THRESHOLD", "vdbe_trace"] {
            for (suffix, call) in [("", false), (" = 1", false), ("(1)", true)] {
                let sql = format!("PRAGMA [main].[{name}]{suffix}");
                let Stmt::Pragma(pragma) = SqliteSynBuilder.parse_as_stmt(&sql) else {
                    panic!("expected pragma");
                };
                let mut gate = Gate::default();
                let qualified = pragma.name(&mut gate).unwrap();
                assert_eq!(qualified.name().value(), name);
                assert_eq!(*qualified.name().quote(), None);
                assert_eq!(qualified.db_name().unwrap().value(), "main");
                assert_eq!(gate.0, if name == "vdbe_trace" { 0 } else { 1 });
                if suffix.is_empty() {
                    assert!(pragma.body().is_none());
                } else {
                    let expression = match pragma.body().as_ref().unwrap() {
                        PragmaBody::Equals(expr) => {
                            assert!(!call);
                            expr
                        }
                        PragmaBody::Call(expr) => {
                            assert!(call);
                            expr
                        }
                    };
                    let Expr::Literal(Literal::Numeric(number)) = expression.as_ref() else {
                        panic!("expected numeric pragma value");
                    };
                    assert_eq!(number.value_text(), "1");
                }
            }
        }
    }
}
