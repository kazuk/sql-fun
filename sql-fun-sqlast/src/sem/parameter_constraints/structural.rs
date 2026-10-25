use crate::sem::{
    ColumnCollection, ColumnName, InputTypeConstraint, ParseContext, TypeDefinition, TypeReference,
};

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
    ArrayItem(ArrayItemProjection),
    ConstructArray,
    Array,
    Member {
        name: String,
        definitions: Vec<(TypeReference, TypeReference)>,
    },
}

/// Context-prepared semantic element relationships, retained beyond analysis.
#[derive(Clone, Debug)]
pub(crate) struct ArrayItemProjection(Vec<(TypeReference, TypeReference)>);
impl ArrayItemProjection {
    fn retain(&mut self, source: &TypeReference, element: &TypeReference) {
        if !self.0.iter().any(|(ty, _)| ty == source) {
            self.0.push((source.clone(), element.clone()));
        }
    }
    fn project(&self, source: &TypeReference) -> Option<TypeReference> {
        if let Some((_, element)) = self.0.iter().find(|(ty, _)| ty == source) {
            return Some(element.clone());
        }
        if source.is_array() == Some(true) {
            Some(source.element_type())
        } else {
            None
        }
    }
    fn preimages(&self, result: &TypeReference) -> Vec<TypeReference> {
        let mut sources: Vec<_> = self
            .0
            .iter()
            .filter(|(_, element)| element == result)
            .map(|(ty, _)| ty.clone())
            .collect();
        if result.is_array() != Some(true) {
            sources.push(result.array_type());
        }
        sources
    }
}

#[cfg(test)]
mod test_array_item_projection {
    use super::{ArrayItemProjection, TypeProjection};
    use crate::sem::{
        ConstraintGraph, InputTypeConstraint, ParameterConstraints, PgBuiltInType, TypeDefinition,
        TypeReference,
    };
    use std::collections::BTreeMap;

    #[test]
    fn retained_vector_identity_is_an_alternative_to_an_ordinary_array() {
        let vector = TypeReference::from_master_type_name("int2vector");
        let element = PgBuiltInType::int2();
        let mut facts = ArrayItemProjection(Vec::new());
        facts.retain(&vector, &element);
        assert_eq!(facts.project(&vector), Some(element.clone()));
        assert!(facts.preimages(&element).contains(&vector));
        assert!(facts.preimages(&element).contains(&element.array_type()));
    }

    #[test]
    fn dynamic_array_relationship_survives_analysis_ownership() {
        let element = PgBuiltInType::int4();
        let array = TypeReference::dynamic(&TypeDefinition::array(&element));
        let mut graph = ConstraintGraph::default();
        let source = graph.parameter(1);
        let result = graph.parameter(2);
        graph.project(
            source,
            result,
            TypeProjection::array_item(&crate::test_helpers::TestParseContext::default()),
        );
        graph.restrict(source, &InputTypeConstraint::exact(&array));
        graph.solve().unwrap();
        let constraints = ParameterConstraints::from_graph(&graph);
        assert!(constraints.allows(&BTreeMap::from([(1, array.clone()), (2, element)])));
        assert!(!constraints.allows(&BTreeMap::from([(1, array), (2, PgBuiltInType::text())])));
    }
}

impl TypeProjection {
    pub(crate) fn array_item(context: &impl ParseContext) -> Self {
        let mut facts = ArrayItemProjection(Vec::new());
        for source in context.type_references() {
            let base = crate::sem::MinMaxExpr::base_type(context, &source);
            if let Some(element) = context.get_array_item_type(&base) {
                facts.retain(&source, &element);
            }
        }
        Self::ArrayItem(facts)
    }

    pub(super) fn preimages(&self, result: &TypeReference) -> Vec<TypeReference> {
        if let Self::ArrayItem(facts) = self {
            facts.preimages(result)
        } else {
            self.inverse(result).into_iter().collect()
        }
    }

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
        if let Self::ArrayItem(facts) = self
            && source.is_array() == Some(true)
        {
            facts.retain(source, &source.element_type());
        }
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
        match source.type_definition() {
            Some(TypeDefinition::CompositeType(definition)) => definition
                .name()
                .map(|name| TypeReference::concrete_type_ref(name, false))
                .unwrap_or_else(|| source.clone()),
            _ => source.clone(),
        }
    }

    fn prepare(&mut self, context: &impl ParseContext, source: &TypeReference) {
        if let Self::ArrayItem(facts) = self {
            let base = crate::sem::MinMaxExpr::base_type(context, source);
            if let Some(element) = context.get_array_item_type(&base) {
                facts.retain(source, &element);
            }
            return;
        }
        if !matches!(self, Self::Member { .. }) || source.is_array() != Some(false) {
            return;
        }
        if source.is_dynamic() {
            self.retain_owned(source);
        } else if let Some(definition) = context.get_type(source.full_name()) {
            // Table row definitions reference their catalog table. Materialize
            // its columns only in this inference-owned representation.
            let owned = if let TypeDefinition::TableRef(table, _) = definition {
                let Some(table) = context.get_table(table) else {
                    return;
                };
                let columns = (0..)
                    .map_while(|index| table.get_column_at(index).cloned())
                    .collect();
                TypeReference::dynamic(&TypeDefinition::ImplicitComposite(ColumnCollection::new(
                    columns,
                )))
            } else {
                TypeReference::dynamic(definition)
            };
            if let Self::Member { definitions, .. } = self
                && !definitions.iter().any(|(ty, _)| ty == source)
            {
                definitions.push((source.clone(), owned));
            }
        }
    }

    pub(super) fn project(&self, source: &TypeReference) -> Option<TypeReference> {
        if let Self::ArrayItem(facts) = self {
            return facts.project(source);
        }
        if let Self::Member { name, definitions } = self {
            if source.is_array() != Some(false) {
                return None;
            }
            let owned = definitions
                .iter()
                .find(|(ty, _)| ty == source)
                .map(|(_, owned)| owned)
                .unwrap_or(source);
            return match owned.type_definition()? {
                TypeDefinition::CompositeType(definition) => {
                    definition.get_member(name)?.get_type()
                }
                TypeDefinition::ImplicitComposite(columns) => columns
                    .get_column_def(&ColumnName::from(name.as_str()))?
                    .get_type_cloned(),
                _ => None,
            };
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
            Self::ArrayItem(_) => unreachable!("semantic element projection handled above"),
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
            Self::ArrayElement if result.is_array() == Some(false) => {
                Some(TypeReference::concrete_type_ref(result.full_name(), true))
            }
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
                let problem = if !source.is_dynamic()
                    && (source.is_array() == Some(true)
                        || context
                            .get_type(source.full_name())
                            .is_none_or(|definition| {
                                !matches!(
                                    definition,
                                    TypeDefinition::CompositeType(_)
                                        | TypeDefinition::TableRef(_, _)
                                        | TypeDefinition::ImplicitComposite(_)
                                )
                            })) {
                    crate::sem::AnalysisProblem::member_access_requires_composite_type(
                        source.full_name(),
                        name,
                    )
                } else {
                    crate::sem::AnalysisProblem::composite_type_field_not_found(source, name)
                };
                context.report_analysis_problem(problem)?;
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
        let array = TypeReference::concrete_type_ref(PgBuiltInType::int4().full_name(), true);
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
        let array = TypeReference::concrete_type_ref(PgBuiltInType::int4().full_name(), true);
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
