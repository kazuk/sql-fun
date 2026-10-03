use crate::syn::sqlite::{Expr, QualifiedName};
use crate::{
    engine_specific::{
        TursoFtsMergeThresholdFeature, TursoMvccGcFeature, TursoMvccGroupCommitFeature,
    },
    sem::AnalysisError,
};

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

    /// Pragma name, notifying applicable Turso-specific pragma hooks.
    pub fn name<
        Gate: TursoMvccGcFeature + TursoMvccGroupCommitFeature + TursoFtsMergeThresholdFeature,
    >(
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
        if self
            .name
            .name()
            .value()
            .eq_ignore_ascii_case("mvcc_group_commit")
        {
            gate.mvcc_group_commit_used(self)?;
        }
        if self
            .name
            .name()
            .value()
            .eq_ignore_ascii_case("fts_merge_threshold")
        {
            gate.fts_merge_threshold_used(self)?;
        }
        Ok(&self.name)
    }
}

use ::turso_parser::ast::PragmaBody as PragmaBodyIn;

/// pragma statement body
#[derive(Debug)]
pub enum PragmaBody {
    /// equals
    Equals(Box<Expr>),
    /// call
    Call(Box<Expr>),
}

impl PragmaBody {
    /// Construct an equals body from an owned expression.
    pub fn equals(expr: Expr) -> Self {
        Self::Equals(Box::new(expr))
    }
    /// Construct a call body from an owned expression.
    pub fn call(expr: Expr) -> Self {
        Self::Call(Box::new(expr))
    }

    /// Borrow the value expression without losing the body spelling.
    pub fn expression(&self) -> &Expr {
        match self {
            Self::Equals(expr) | Self::Call(expr) => expr,
        }
    }
    /// Whether the body uses call syntax rather than equals syntax.
    pub const fn is_call(&self) -> bool {
        matches!(self, Self::Call(_))
    }
}

impl From<PragmaBodyIn> for PragmaBody {
    fn from(value: PragmaBodyIn) -> Self {
        match value {
            PragmaBodyIn::Equals(expr) => Self::equals(Expr::from(*expr)),
            PragmaBodyIn::Call(expr) => Self::call(Expr::from(*expr)),
        }
    }
}

#[cfg(test)]
mod test_new_pragmas {
    use super::{
        AnalysisError, PragmaStatement, TursoFtsMergeThresholdFeature, TursoMvccGcFeature,
        TursoMvccGroupCommitFeature,
    };
    use crate::test_helpers::SqliteSynBuilder;

    #[derive(Default)]
    struct Gate {
        gc: usize,
        group_commit: usize,
        fts_merge: usize,
    }
    impl TursoMvccGcFeature for Gate {
        fn mvcc_gc_threshold_used(&mut self, _: &PragmaStatement) -> Result<(), AnalysisError> {
            self.gc += 1;
            Ok(())
        }
    }

    impl TursoMvccGroupCommitFeature for Gate {
        fn mvcc_group_commit_used(&mut self, _: &PragmaStatement) -> Result<(), AnalysisError> {
            self.group_commit += 1;
            Ok(())
        }
    }
    impl TursoFtsMergeThresholdFeature for Gate {
        fn fts_merge_threshold_used(&mut self, _: &PragmaStatement) -> Result<(), AnalysisError> {
            self.fts_merge += 1;
            Ok(())
        }
    }

    #[rstest::rstest]
    fn generic_pragma_preserves_names_and_bodies(
        #[values(
            "mvcc_gc_threshold",
            "MVCC_GC_THRESHOLD",
            "vdbe_trace",
            "count_changes",
            "mvcc_group_commit",
            "MVCC_GROUP_COMMIT",
            "fts_merge_threshold",
            "FTS_MERGE_THRESHOLD"
        )]
        name: &str,
        #[values(("", false), (" = 1", false), ("(1)", true))] body_case: (&str, bool),
    ) {
        let (suffix, call) = body_case;
        let sql = format!("PRAGMA [main].[{name}]{suffix}");
        let statement = SqliteSynBuilder.parse_as_stmt(&sql);
        let pragma = statement.as_pragma().expect("pragma");
        let mut gate = Gate::default();
        let qualified = pragma.name(&mut gate).unwrap();
        assert_eq!(qualified.name().value(), name);
        assert_eq!(*qualified.name().quote(), Some('['));
        assert_eq!(qualified.db_name().unwrap().value(), "main");
        assert_eq!(
            gate.gc,
            usize::from(name.eq_ignore_ascii_case("mvcc_gc_threshold"))
        );
        assert_eq!(
            gate.group_commit,
            usize::from(name.eq_ignore_ascii_case("mvcc_group_commit"))
        );
        assert_eq!(
            gate.fts_merge,
            usize::from(name.eq_ignore_ascii_case("fts_merge_threshold"))
        );
        if suffix.is_empty() {
            assert!(pragma.body().is_none());
        } else {
            let body = pragma.body().as_ref().unwrap();
            assert_eq!(body.is_call(), call);
            assert_eq!(body.expression().numeric_literal_text(), Some("1"));
        }
    }
}
