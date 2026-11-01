use crate::sem::{ColumnName, InputTypeConstraint, ParseContext, TypeDefinition, TypeReference};

/// A functional type relationship, evaluated in the graph's common fixed point.
#[derive(Clone, Debug)]
pub(super) struct StructuralConstraint {
    source: usize,
    result: usize,
    projection: TypeProjection,
}

#[derive(Clone, Debug)]
pub(crate) enum TypeProjection {
    ArrayElement,
    ConstructArray,
    Array,
    Member {
        name: String,
        definitions: Vec<(TypeReference, TypeReference)>,
    },
}

impl TypeProjection {
    pub(crate) const fn construct_array() -> Self {
        Self::ConstructArray
    }

    pub(crate) const fn array_element() -> Self {
        Self::ArrayElement
    }

    pub(crate) const fn array() -> Self {
        Self::Array
    }

    pub(crate) fn member(name: &str) -> Self {
        Self::Member {
            name: name.to_owned(),
            definitions: Vec::new(),
        }
    }

    pub(super) fn retain_owned(&mut self, source: &TypeReference) {
        if let Self::Member { definitions, .. } = self
            && source.type_definition().is_some()
        {
            let key = Self::identity(source);
            if !definitions.iter().any(|(ty, _)| ty == &key) {
                definitions.push((key, source.clone()));
            }
        }
    }

    pub(super) fn identity(source: &TypeReference) -> TypeReference {
        if let Some(definition) = source.type_definition()
            && (definition.is_composite_type() || definition.is_domain())
            && let Some(reference) = definition.type_reference()
        {
            reference.clone()
        } else {
            source.clone()
        }
    }

    fn prepare(&mut self, context: &impl ParseContext, source: &TypeReference) {
        if !matches!(self, Self::Member { .. }) || source.is_array() != Some(false) {
            return;
        }
        if let Some(columns) = source.row_columns(context) {
            // Resolve domain/table row fields while the catalog is available,
            // retaining the source identity for correlated input assignments.
            let owned = TypeReference::dynamic(&TypeDefinition::implicit_composite(&columns));
            if let Self::Member { definitions, .. } = self {
                if let Some((_, retained)) = definitions.iter_mut().find(|(ty, _)| ty == source) {
                    *retained = owned;
                } else {
                    definitions.push((source.clone(), owned));
                }
            }
        }
    }

    pub(super) fn project(&self, source: &TypeReference) -> Option<TypeReference> {
        if let Self::Member { name, definitions } = self {
            if source.is_array() != Some(false) {
                return None;
            }
            let owned = definitions
                .iter()
                .find(|(ty, _)| ty == source)
                .map(|(_, owned)| owned)
                .unwrap_or(source);
            return owned
                .owned_row_columns()?
                .get_column_def(&ColumnName::from(name.as_str()))?
                .get_type_cloned();
        }
        if matches!(self, Self::ConstructArray) {
            return Some(source.array_type());
        }
        if source.is_dynamic() || source.is_array() != Some(true) {
            return None;
        }
        match self {
            Self::ConstructArray => unreachable!("array construction handled above"),
            Self::ArrayElement => Some(source.element_type()),
            Self::Array => Some(source.clone()),
            Self::Member { .. } => unreachable!("member projection handled above"),
        }
    }

    pub(super) fn inverse(&self, result: &TypeReference) -> Option<TypeReference> {
        if matches!(self, Self::ConstructArray) && result.is_array() == Some(true) {
            return Some(result.element_type());
        }
        if result.is_dynamic() {
            return None;
        }
        match self {
            Self::ArrayElement if result.is_array() == Some(false) => Some(result.array_type()),
            Self::Array if result.is_array() == Some(true) => Some(result.clone()),
            _ => None,
        }
    }
}

impl StructuralConstraint {
    pub(super) const fn new(source: usize, result: usize, projection: TypeProjection) -> Self {
        Self {
            source,
            result,
            projection,
        }
    }

    pub(super) fn prepare(
        &mut self,
        context: &impl ParseContext,
        source: &TypeReference,
    ) -> Option<TypeReference> {
        self.projection.prepare(context, source);
        self.projection.project(source)
    }

    pub(super) fn retain_owned(&mut self, source: &TypeReference) {
        self.projection.retain_owned(source);
    }

    pub(super) fn report_problem(
        &self,
        domains: &[InputTypeConstraint],
        context: &mut impl ParseContext,
    ) -> Result<(), crate::sem::AnalysisError> {
        if let TypeProjection::Member { name, .. } = &self.projection
            && let Some(sources) = domains[self.source].admissible_types()
            && sources
                .iter()
                .all(|source| self.projection.project(source).is_none())
        {
            for source in sources {
                if let Some(problem) = source.member_access_problem(context, name) {
                    context.report_analysis_problem(problem)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn propagate(&self, domains: &mut [InputTypeConstraint]) -> bool {
        if !domains[self.source].is_any() {
            let mut sources = domains[self.source].clone();
            sources.retain_projection(&self.projection, &domains[self.result]);
            let results = sources.projected(&self.projection);
            let mut changed = domains[self.source].intersect(&sources);
            changed |= domains[self.result].intersect(&results);
            changed
        } else if !domains[self.result].is_any()
            && !matches!(self.projection, TypeProjection::Member { .. })
        {
            let mut sources = domains[self.result].preimage(&self.projection);
            if matches!(self.projection, TypeProjection::ConstructArray) {
                // ARRAY[array_value] has the same SQL array identity. Its source
                // may therefore be either an element or an already-array value.
                sources.union(&domains[self.result]);
            }
            domains[self.source].intersect(&sources)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TypeProjection;
    use crate::sem::{
        ConstraintGraph, InputTypeConstraint, ParameterConstraints, PgBuiltInType, TypeReference,
    };
    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::forward(true)]
    #[case::reverse(false)]
    fn array_element_propagates_in_both_directions(#[case] forward: bool) {
        let mut graph = ConstraintGraph::default();
        let source = graph.parameter(1);
        let result = graph.parameter(2);
        let array = PgBuiltInType::int4().array_type();
        graph.project(source, result, TypeProjection::array_element());
        if forward {
            graph.restrict(source, &InputTypeConstraint::exact(&array));
        } else {
            graph.restrict(result, &InputTypeConstraint::exact(&PgBuiltInType::int4()));
        }
        graph.solve().unwrap();
        assert_eq!(graph.domain(source), &InputTypeConstraint::exact(&array));
        assert_eq!(
            graph.domain(result),
            &InputTypeConstraint::exact(&PgBuiltInType::int4())
        );
    }

    #[test]
    fn unbounded_relationship_is_checked_when_concrete_inputs_arrive() {
        let mut graph = ConstraintGraph::default();
        let source = graph.parameter(1);
        let result = graph.parameter(2);
        graph.project(source, result, TypeProjection::array_element());
        graph.solve().unwrap();
        let constraints = ParameterConstraints::from_graph(&graph);
        assert!(constraints.parameter(1).unwrap().constraint().is_any());
        let array = PgBuiltInType::int4().array_type();
        assert!(constraints.allows(&BTreeMap::from([
            (1, array.clone()),
            (2, PgBuiltInType::int4())
        ])));
        assert!(!constraints.allows(&BTreeMap::from([(1, array), (2, PgBuiltInType::text())])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
    }
    fn record(field_type: &TypeReference) -> TypeReference {
        use crate::sem::{ColumnCollection, ColumnDefinition, ColumnName, TypeDefinition};
        TypeReference::dynamic(&TypeDefinition::ImplicitComposite(ColumnCollection::new(
            vec![ColumnDefinition::new(
                &Some(ColumnName::from("n")),
                Some(field_type),
                None,
                &[],
            )],
        )))
    }

    #[test]
    fn member_candidates_and_results_remain_correlated() {
        let mut graph = ConstraintGraph::default();
        let source = graph.parameter(1);
        let result = graph.parameter(2);
        let integer_record = record(&PgBuiltInType::int4());
        let text_record = record(&PgBuiltInType::text());
        graph.restrict(
            source,
            &InputTypeConstraint::types([integer_record.clone(), text_record.clone()]),
        );
        graph.project(source, result, TypeProjection::member("n"));
        graph.solve().unwrap();
        let constraints = ParameterConstraints::from_graph(&graph);
        assert_eq!(
            constraints
                .parameter(1)
                .unwrap()
                .constraint()
                .admissible_types()
                .unwrap()
                .len(),
            2
        );
        assert!(constraints.allows(&BTreeMap::from([
            (1, integer_record.clone()),
            (2, PgBuiltInType::int4())
        ])));
        assert!(constraints.allows(&BTreeMap::from([
            (1, text_record),
            (2, PgBuiltInType::text())
        ])));
        assert!(!constraints.allows(&BTreeMap::from([
            (1, integer_record),
            (2, PgBuiltInType::text())
        ])));
    }

    #[test]
    fn member_result_filters_existing_candidates() {
        let mut graph = ConstraintGraph::default();
        let source = graph.parameter(1);
        let result = graph.parameter(2);
        let integer_record = record(&PgBuiltInType::int4());
        graph.restrict(
            source,
            &InputTypeConstraint::types([integer_record.clone(), record(&PgBuiltInType::text())]),
        );
        graph.restrict(result, &InputTypeConstraint::exact(&PgBuiltInType::int4()));
        graph.project(source, result, TypeProjection::member("n"));
        graph.solve().unwrap();
        assert_eq!(
            graph.domain(source),
            &InputTypeConstraint::exact(&integer_record)
        );
    }
}
