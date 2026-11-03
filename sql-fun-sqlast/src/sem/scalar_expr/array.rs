use crate::{
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, InputTypeConstraint, MinMaxExpr, ParameterConstraints,
        SemScalarExpr, TypeReference,
        parameter_resolution::ParameterResolution,
        problem::ArrayProblem,
        scalar_expr::SemScalarExprNode,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// An analyzed ARRAY constructor with ordered operands and its output domain.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArrayExpr {
    values: Vec<SemScalarExpr>,
    types: InputTypeConstraint,
    explicit_target: Option<TypeReference>,
    multidimensional: bool,
    #[serde(skip)]
    node: Option<usize>,
}

impl ArrayExpr {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        values: &[SemScalarExpr],
        target: Option<&TypeReference>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut value = Self {
            values: values.to_vec(),
            types: InputTypeConstraint::any(),
            explicit_target: target.cloned(),
            multidimensional: values.iter().any(|value| {
                value.as_array().is_some()
                    || MinMaxExpr::input_type(value).is_some_and(|ty| ty.is_array() == Some(true))
            }),
            node: None,
        };
        if values.is_empty() && target.is_none() {
            value.fail(context, ArrayProblem::empty())?;
        } else if let Some(target) = target {
            let element = value.operand_target(target);
            if let Some(source) = values
                .iter()
                .filter_map(MinMaxExpr::input_type)
                .find(|source| !Self::can_explicitly_coerce(context, source, &element))
            {
                value.fail(context, ArrayProblem::incompatible(&source, &element))?;
            } else {
                value.types = InputTypeConstraint::exact(target);
                value.coerce(target, false);
            }
        } else if values
            .iter()
            .all(|value| MinMaxExpr::input_type(value).is_some())
        {
            let mut inputs: Vec<_> = values.iter().filter_map(MinMaxExpr::input_type).collect();
            if !inputs.windows(2).all(|pair| pair[0] == pair[1]) {
                inputs = inputs.iter().map(Self::selection_type).collect();
            }
            match MinMaxExpr::select_common_type(context, &inputs) {
                Ok(element) => {
                    let target = element.array_type();
                    value.types = InputTypeConstraint::exact(&target);
                    value.coerce(&target, false);
                }
                Err(problem) => {
                    let Ok(problem) = ArrayProblem::try_from(problem) else {
                        return AnalysisError::raise_unexpected_input(
                            "comparison failure in ARRAY common-type selection",
                        );
                    };
                    value.fail(context, problem)?;
                }
            }
        }
        Ok(SemScalarExpr::analyzed_array(&value))
    }

    fn fail(
        &mut self,
        context: &mut impl ParseContext,
        problem: ArrayProblem,
    ) -> Result<(), AnalysisError> {
        self.types = InputTypeConstraint::types([]);
        context.report_analysis_problem(AnalysisProblem::array(problem))
    }

    /// Ordered analyzed elements, including recorded element coercions.
    #[must_use]
    pub fn values(&self) -> &[SemScalarExpr] {
        &self.values
    }

    /// Output types: `any` while unresolved, empty on failure, or analyzed candidates.
    #[must_use]
    pub const fn types(&self) -> &InputTypeConstraint {
        &self.types
    }

    pub(crate) const fn node(&self) -> Option<usize> {
        self.node
    }

    fn operand_target(&self, array: &TypeReference) -> TypeReference {
        if self.multidimensional {
            array.clone()
        } else {
            array.element_type()
        }
    }

    // Anonymous row descriptors refine values, but PostgreSQL chooses the
    // common nominal record type when their descriptors differ.
    fn selection_type(ty: &TypeReference) -> TypeReference {
        if !ty.is_dynamic() {
            return ty.clone();
        }
        if ty.is_array() == Some(true) {
            Self::selection_type(&ty.element_type()).array_type()
        } else if ty.is_record() {
            TypeReference::from_master_type_name("record")
        } else {
            ty.clone()
        }
    }

    fn constraint_values(&self) -> Vec<SemScalarExpr> {
        let mut values = Vec::with_capacity(self.values.len());
        for value in &self.values {
            let ty = MinMaxExpr::input_type(value);
            if let Some(ty) = ty
                && ty.is_dynamic()
            {
                values.push(SemScalarExpr::wrap_implicit_cast(
                    &Self::selection_type(&ty),
                    value,
                ));
            } else {
                values.push(value.clone());
            }
        }
        values
    }

    fn can_explicitly_coerce(
        context: &impl ParseContext,
        source: &TypeReference,
        target: &TypeReference,
    ) -> bool {
        if Self::selection_type(source) == Self::selection_type(target)
            || MinMaxExpr::can_coerce(context, source, target)
            || (!source.is_dynamic()
                && !target.is_dynamic()
                && context.get_explicit_cast(source, target).is_some())
        {
            return true;
        }
        if source.is_array() == Some(true) && target.is_array() == Some(true) {
            return Self::can_explicitly_coerce(
                context,
                &source.element_type(),
                &target.element_type(),
            );
        }
        false
    }

    fn coerce(&mut self, target: &TypeReference, finalized: bool) {
        let element = self.operand_target(target);
        for value in &mut self.values {
            // A pending parameter must remain visible to the constraint collector.
            let pending =
                !finalized && value.constraint_node().is_some() && value.get_type().is_none();
            let needs_coercion = value.get_type().as_ref() != Some(&element)
                || MinMaxExpr::input_type(value) == Some(MinMaxExpr::unknown());
            if !pending && needs_coercion {
                *value = if self.explicit_target.is_some() {
                    SemScalarExpr::explicit_cast(value, &element)
                } else {
                    SemScalarExpr::wrap_implicit_cast(&element, value)
                };
            }
        }
    }

    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for value in &mut self.values {
            value.walk(visitor)?;
        }
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        for value in &self.values {
            value.visit(visitor)?;
        }
        Ok(())
    }

    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if self.types.admissible_types() == Some(&[]) {
            return Ok(());
        }
        if self
            .values
            .iter()
            .any(|value| value.constraint_node().is_some())
        {
            let (node, types) = parameters.array(
                context,
                &self.constraint_values(),
                self.explicit_target.as_ref(),
                self.multidimensional,
            )?;
            self.node = Some(node);
            self.types = types;
        }
        Ok(())
    }

    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        if let Some(node) = self.node {
            self.types = constraints.node_domain(node).clone();
            if let Some(target) = self.types.single_type() {
                self.coerce(&target, true);
            }
        }
    }

    pub(super) const fn canonicalize_grouping_key(&mut self) {
        self.node = None;
    }
}

impl SemScalarExprNode for ArrayExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.types.single_type()
    }
    fn is_not_null(&self) -> Option<bool> {
        if self.types.admissible_types() == Some(&[]) {
            None
        } else {
            Some(true)
        }
    }
}
