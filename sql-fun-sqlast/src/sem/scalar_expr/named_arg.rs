use crate::StringSpan;
use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{AnalysisError, SemScalarExpr, TypeReference};

/// A supplied PostgreSQL argument name and its analyzed value.
/// The name follows SQL identifier normalization; it is not a formal binding.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NamedArgExpr {
    name: String,
    value: Box<SemScalarExpr>,
    source_span: Option<StringSpan>,
}

impl NamedArgExpr {
    /// Preserve a supplied name, value, and reliable source span of the name.
    pub fn new(name: &str, value: &SemScalarExpr, source_span: &Option<StringSpan>) -> Self {
        Self {
            name: name.to_owned(),
            value: Box::new(value.clone()),
            source_span: *source_span,
        }
    }
    /// Supplied argument name, independent of any function signature.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Analyzed argument value, without signature-derived casts or defaults.
    pub fn value(&self) -> &SemScalarExpr {
        &self.value
    }
    /// Source span of the supplied name, when provided by the parser.
    pub const fn source_span(&self) -> Option<&StringSpan> {
        self.source_span.as_ref()
    }
    pub(crate) const fn canonicalize_grouping_key(&mut self) {
        self.source_span = None;
    }
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.value.walk(visitor)
    }
}

impl SemScalarExprNode for NamedArgExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.value.get_type()
    }
    fn is_not_null(&self) -> Option<bool> {
        self.value.is_not_null()
    }
}
