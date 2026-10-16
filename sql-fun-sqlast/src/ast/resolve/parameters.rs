//! Analysis-local graph building and replay. Source AST and catalog models stay intact.
use crate::ast::Expr;
use crate::sem::{
    AnalysisError, Catalog, ConstrainedExpr, ConstraintGraph, InputTypeConstraint,
    ParameterConstraints, SemScalarExpr, SignatureAlternative, TypeReference,
};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Default)]
struct QueryScope {
    ctes: BTreeMap<String, Vec<Output>>,
    sources: BTreeMap<String, Vec<Output>>,
    outputs: Vec<Output>,
}

#[derive(Clone)]
struct Output {
    name: String,
    node: Option<usize>,
    domain: InputTypeConstraint,
}

struct Expression {
    operation: String,
    arguments: Vec<SemScalarExpr>,
    position: Option<u32>,
    not_null: Option<bool>,
    name: String,
}

struct CachedExpression {
    node: usize,
    domain: InputTypeConstraint,
    name: String,
    not_null: Option<bool>,
}

#[derive(Default)]
pub(crate) struct ParameterResolution {
    graph: ConstraintGraph,
    expressions: HashMap<usize, CachedExpression>,
    values: BTreeMap<usize, Expression>,
    scopes: Vec<QueryScope>,
    completed: Vec<Output>,
    replay: bool,
}

impl ParameterResolution {
    pub(crate) fn constraints(&self) -> ParameterConstraints {
        let mut constraints = ParameterConstraints::from_graph(&self.graph);
        let outputs: Vec<_> = self
            .completed
            .iter()
            .map(|output| {
                output
                    .node
                    .filter(|node| !self.graph.domain(*node).is_any())
                    .map(|node| self.graph.domain(node).clone())
                    .unwrap_or_else(|| output.domain.clone())
            })
            .collect();
        constraints.set_outputs(&outputs);
        constraints
    }

    pub(crate) fn is_any(&self, value: &SemScalarExpr) -> bool {
        value
            .constraint_node()
            .is_some_and(|node| self.graph.domain(node).is_any())
    }

    pub(crate) fn solve(&mut self) -> Result<bool, AnalysisError> {
        if self.values.is_empty() {
            return Ok(false);
        }
        self.graph.solve()?;
        self.replay = true;
        self.scopes.clear();
        self.completed.clear();
        Ok(true)
    }

    pub(crate) fn cached(&self, expression: &Expr) -> Option<SemScalarExpr> {
        if !self.replay {
            return None;
        }
        self.expressions
            .get(&(std::ptr::from_ref(expression) as usize))
            .map(|cached| {
                let value = self.materialize(cached.node);
                if cached.name.is_empty() {
                    value
                } else {
                    SemScalarExpr::constrained(&ConstrainedExpr::new(
                        cached.node,
                        "column",
                        &[value],
                        if self.graph.domain(cached.node).is_any() {
                            &cached.domain
                        } else {
                            self.graph.domain(cached.node)
                        },
                        cached.not_null,
                        &cached.name,
                    ))
                }
            })
    }

    pub(crate) fn remember(&mut self, expression: &Expr, value: &SemScalarExpr) {
        if let Some(node) = value.constraint_node() {
            self.expressions.insert(
                std::ptr::from_ref(expression) as usize,
                CachedExpression {
                    node,
                    domain: value
                        .get_type()
                        .as_ref()
                        .map(InputTypeConstraint::exact)
                        .unwrap_or_else(InputTypeConstraint::any),
                    name: value.get_column_name(),
                    not_null: value.is_not_null(),
                },
            );
        }
    }

    fn materialize(&self, node: usize) -> SemScalarExpr {
        crate::sem::with_stack(|| {
            let value = &self.values[&node];
            let domain = self.graph.domain(node);
            if let Some(position) = value.position {
                return SemScalarExpr::parameter(position, node, domain);
            }
            let arguments: Vec<_> = value
                .arguments
                .iter()
                .map(|argument| {
                    if let Some(node) = argument.constraint_node() {
                        self.materialize(node)
                    } else {
                        argument.clone()
                    }
                })
                .collect();
            SemScalarExpr::constrained(&ConstrainedExpr::new(
                node,
                &value.operation,
                &arguments,
                domain,
                value.not_null,
                &value.name,
            ))
        })
    }

    pub(crate) fn parameter(&mut self, position: u32) -> Result<SemScalarExpr, AnalysisError> {
        let node = self.graph.parameter(position)?;
        self.values.entry(node).or_insert_with(|| Expression {
            operation: "parameter".to_owned(),
            arguments: Vec::new(),
            position: Some(position),
            not_null: None,
            name: String::new(),
        });
        Ok(SemScalarExpr::parameter(
            position,
            node,
            self.graph.domain(node),
        ))
    }

    fn input(&mut self, value: &SemScalarExpr) -> usize {
        value.constraint_node().unwrap_or_else(|| {
            self.graph.node(
                &value
                    .get_type()
                    .as_ref()
                    .map(InputTypeConstraint::exact)
                    .unwrap_or_else(InputTypeConstraint::any),
            )
        })
    }

    pub(crate) fn operation(
        &mut self,
        operation: &str,
        arguments: &[SemScalarExpr],
        domain: &InputTypeConstraint,
        not_null: Option<bool>,
    ) -> SemScalarExpr {
        let node = self.graph.node(domain);
        self.values.insert(
            node,
            Expression {
                operation: operation.to_owned(),
                arguments: arguments.to_vec(),
                position: None,
                not_null,
                name: String::new(),
            },
        );
        self.materialize(node)
    }

    pub(crate) fn require(
        &mut self,
        catalog: &dyn Catalog,
        value: &SemScalarExpr,
        target: &TypeReference,
        explicit: bool,
    ) {
        if let Some(node) = value.constraint_node() {
            self.graph.restrict(
                node,
                &InputTypeConstraint::types(catalog.cast_sources(target, explicit)),
            );
        }
    }

    pub(crate) fn call(
        &mut self,
        catalog: &dyn Catalog,
        name: &str,
        arguments: &[SemScalarExpr],
        signatures: &[Signature],
    ) -> Result<SemScalarExpr, AnalysisError> {
        let result = self.operation(name, arguments, &InputTypeConstraint::any(), None);
        let mut nodes: Vec<_> = arguments
            .iter()
            .map(|argument| self.input(argument))
            .collect();
        nodes.push(result.constraint_node().expect("operation has node"));
        let domains: Vec<_> = nodes[..arguments.len()]
            .iter()
            .map(|node| self.graph.domain(*node).clone())
            .collect();
        let alternatives = Signature::alternatives(signatures, catalog, &domains);
        self.graph.overload(&nodes, &alternatives, name);
        Ok(result)
    }

    pub(crate) fn scalar_result(&mut self, value: &SemScalarExpr) -> Option<SemScalarExpr> {
        let [output] = self.completed.as_slice() else {
            return None;
        };
        let source = output.node?;
        let result = self.operation(
            "scalar subquery",
            std::slice::from_ref(value),
            &InputTypeConstraint::any(),
            value.is_not_null(),
        );
        self.graph
            .equal(source, result.constraint_node().expect("operation node"));
        Some(result)
    }

    pub(crate) fn retain_completed_outputs(&mut self) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.outputs = self.completed.clone();
        }
    }

    pub(crate) fn enter_query(&mut self) {
        let ctes = self
            .scopes
            .last()
            .map(|scope| scope.ctes.clone())
            .unwrap_or_default();
        self.scopes.push(QueryScope {
            ctes,
            ..QueryScope::default()
        });
    }
    pub(crate) fn leave_query(&mut self) {
        self.completed = self.scopes.pop().expect("query scope").outputs;
    }
    pub(crate) fn output(&mut self, name: &str, value: &SemScalarExpr, text_projection: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.outputs.push(Output {
                name: name.to_owned(),
                node: value.constraint_node(),
                domain: if text_projection {
                    InputTypeConstraint::exact(&crate::sem::PgBuiltInType::text())
                } else {
                    value
                        .get_type()
                        .as_ref()
                        .map(InputTypeConstraint::exact)
                        .unwrap_or_else(InputTypeConstraint::any)
                },
            });
        }
    }
    pub(crate) fn cte(&mut self, name: &str, columns: &[String]) {
        let mut outputs = self.completed.clone();
        for (output, name) in outputs.iter_mut().zip(columns) {
            output.name.clone_from(name);
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.ctes.insert(name.to_owned(), outputs);
        }
    }
    pub(crate) fn relation(&mut self, name: Option<&str>, alias: &str, columns: &[String]) {
        if let Some(scope) = self.scopes.last_mut() {
            let mut outputs = name
                .and_then(|name| scope.ctes.get(name))
                .cloned()
                .unwrap_or_default();
            for (output, name) in outputs.iter_mut().zip(columns) {
                output.name.clone_from(name);
            }
            scope.sources.insert(alias.to_owned(), outputs);
        }
    }
    pub(crate) fn derived(&mut self, alias: &str, columns: &[String]) {
        let mut outputs = self.completed.clone();
        for (output, name) in outputs.iter_mut().zip(columns) {
            output.name.clone_from(name);
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.sources.insert(alias.to_owned(), outputs);
        }
    }
    pub(crate) fn column(&self, original: &SemScalarExpr) -> Option<SemScalarExpr> {
        let (alias, column) = original.constraint_binding()?;
        for scope in self.scopes.iter().rev() {
            if let Some(outputs) = scope.sources.get(alias) {
                let output = outputs
                    .iter()
                    .find(|output| output.name == column.as_str())?;
                let node = output.node?;
                return Some(SemScalarExpr::constrained(&ConstrainedExpr::new(
                    node,
                    "column",
                    &[],
                    if self.graph.domain(node).is_any() {
                        &output.domain
                    } else {
                        self.graph.domain(node)
                    },
                    original.is_not_null(),
                    column.as_str(),
                )));
            }
        }
        None
    }
}

pub(crate) struct Signature {
    inputs: Vec<TypeReference>,
    result: TypeReference,
}
impl Signature {
    pub(crate) fn new(inputs: &[TypeReference], result: &TypeReference) -> Self {
        Self {
            inputs: inputs.to_vec(),
            result: result.clone(),
        }
    }
    // Partition source types by their cast scores against all candidates. Types
    // within one partition have identical overload behavior, so the solver can
    // keep ranges without enumerating every concrete input tuple.
    fn alternatives(
        signatures: &[Self],
        catalog: &dyn Catalog,
        inputs: &[InputTypeConstraint],
    ) -> Vec<SignatureAlternative> {
        let signatures: Vec<_> = signatures
            .iter()
            .filter(|signature| {
                signature.inputs.len() == inputs.len()
                    && signature
                        .inputs
                        .iter()
                        .chain([&signature.result])
                        .all(|ty| {
                            !ty.is_dynamic()
                                && !ty.is_simple_infer_arg()
                                && !ty.is_common_infer_arg()
                        })
            })
            .collect();
        let mut choices = Vec::new();
        for (index, input) in inputs.iter().enumerate() {
            let mut sources = std::collections::HashSet::new();
            for signature in &signatures {
                sources.extend(catalog.cast_sources(&signature.inputs[index], false));
            }
            let mut groups = BTreeMap::<Vec<Option<i32>>, Vec<TypeReference>>::new();
            for source in sources {
                if !input.allows(&source) {
                    continue;
                }
                let scores = signatures
                    .iter()
                    .map(|signature| {
                        catalog
                            .get_implicit_cast(&source, &signature.inputs[index])
                            .map(|cast| cast.context().overload_priority_score())
                    })
                    .collect();
                groups.entry(scores).or_default().push(source);
            }
            choices.push(
                groups
                    .into_iter()
                    .map(|(scores, types)| SourcePartition {
                        scores,
                        domain: InputTypeConstraint::types(types),
                    })
                    .collect::<Vec<_>>(),
            );
        }
        let mut alternatives = Vec::new();
        Self::combine(
            &signatures,
            &choices,
            0,
            &[],
            &vec![Some(0); signatures.len()],
            &mut alternatives,
        );
        alternatives
    }

    fn combine(
        signatures: &[&Self],
        choices: &[Vec<SourcePartition>],
        index: usize,
        domains: &[InputTypeConstraint],
        scores: &[Option<i32>],
        alternatives: &mut Vec<SignatureAlternative>,
    ) {
        if index == choices.len() {
            let Some(best) = scores.iter().flatten().max() else {
                return;
            };
            let mut candidates = scores
                .iter()
                .enumerate()
                .filter(|(_, score)| score.as_ref() == Some(best));
            let Some((candidate, _)) = candidates.next() else {
                return;
            };
            // A concrete source assignment with tied best overloads is ambiguous.
            if candidates.next().is_some() {
                return;
            }
            let mut domains = domains.to_vec();
            domains.push(InputTypeConstraint::exact(&signatures[candidate].result));
            alternatives.push(SignatureAlternative::new(&domains));
            return;
        }
        for choice in &choices[index] {
            let next_scores: Vec<_> = scores
                .iter()
                .zip(&choice.scores)
                .map(|(left, right)| Some((*left)? + (*right)?))
                .collect();
            if next_scores.iter().all(Option::is_none) {
                continue;
            }
            let mut domains = domains.to_vec();
            domains.push(choice.domain.clone());
            crate::sem::with_stack(|| {
                Self::combine(
                    signatures,
                    choices,
                    index + 1,
                    &domains,
                    &next_scores,
                    alternatives,
                )
            });
        }
    }
}

struct SourcePartition {
    scores: Vec<Option<i32>>,
    domain: InputTypeConstraint,
}
