use std::sync::Arc;

use crate::{
    ast::{self, ObjectKind},
    context::ParseContext,
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, CreateFunction, OverloadVariant, OwnershipIdentity,
        OwnershipLookup, SemScalarExpr, SourcePathWithSpan, TypeReference, ViewLike,
        ownership::RequestedIdentity, traversal::SemanticVisitor,
    },
};

/// PostgreSQL DROP FUNCTION and its resolved catalog targets.
#[derive(Debug, Clone)]
pub struct DropFunction {
    targets: Vec<OwnershipIdentity>,
    source: Option<SourcePathWithSpan>,
    cascade: bool,
    dependent_views: Vec<FullName>,
    applicable: bool,
}

impl DropFunction {
    pub(crate) fn analyze(
        ast: &ast::DropFunction,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut targets = Vec::new();
        let mut applicable = true;
        for target in ast.targets() {
            let request =
                RequestedIdentity::from_ast_object_target(target, ObjectKind::FUNCTION, context)?;
            let outcome = match request.resolve(context) {
                // A bare name searches functions only. An explicit signature
                // can instead identify a procedure and report a kind mismatch.
                OwnershipLookup::InvalidKind(identity) if target.argument_types().is_none() => {
                    OwnershipLookup::NotFound(identity)
                }
                outcome => outcome,
            };
            match outcome {
                OwnershipLookup::Resolved(identity) => {
                    let aggregate =
                        context
                            .routine_definitions(identity.name())
                            .iter()
                            .any(|definition| {
                                definition.catalog_aggregate()
                                    && definition.ownership_identity(identity.name()) == identity
                            });
                    if aggregate {
                        applicable = false;
                        context.report_analysis_problem(AnalysisProblem::ownership_target(
                            &OwnershipLookup::invalid_kind(&identity),
                        ))?;
                    } else if !targets.contains(&identity) {
                        targets.push(identity);
                    }
                }
                OwnershipLookup::NotFound(_) | OwnershipLookup::ArgumentTypeNotFound(_)
                    if ast.if_exists() => {}
                OwnershipLookup::NotFound(identity) => {
                    applicable = false;
                    context.report_analysis_problem(AnalysisProblem::function_not_found(
                        identity.name(),
                        &context.get_span().copied().unwrap_or_default(),
                    ))?;
                }
                problem => {
                    applicable = false;
                    context.report_analysis_problem(AnalysisProblem::ownership_target(&problem))?;
                }
            }
        }
        let cascade = ast.behavior() == ast::DropBehavior::Cascade;
        let dependent_views = if applicable && !targets.is_empty() {
            dependent_views(context, &targets)?
        } else {
            Vec::new()
        };
        if !cascade && let Some(name) = dependent_views.first() {
            applicable = false;
            context
                .report_analysis_problem(AnalysisProblem::DropFunctionDependency(name.into()))?;
        }
        Ok(Self {
            targets,
            dependent_views,
            source: context.get_source_path_with_span().cloned(),
            cascade,
            applicable,
        })
    }

    /// Exact qualified function identities selected by the statement.
    pub fn targets(&self) -> &[OwnershipIdentity] {
        &self.targets
    }

    /// Location of the originating DROP FUNCTION statement, when supplied.
    pub const fn source(&self) -> Option<&SourcePathWithSpan> {
        self.source.as_ref()
    }

    /// Whether the statement requested CASCADE.
    pub const fn cascade(&self) -> bool {
        self.cascade
    }

    /// Views that block RESTRICT or will be removed by CASCADE.
    pub fn dependent_views(&self) -> &[FullName] {
        &self.dependent_views
    }

    /// Whether this statement can apply catalog changes.
    pub const fn applicable(&self) -> bool {
        self.applicable && !self.targets.is_empty()
    }
}

/// A deleted function definition and the semantic statement that removed it.
#[derive(Debug, Clone)]
pub struct FunctionTombstone {
    definition: CreateFunction,
    cause: Arc<DropFunction>,
}

impl FunctionTombstone {
    pub(crate) fn new(definition: CreateFunction, cause: &Arc<DropFunction>) -> Self {
        Self {
            definition,
            cause: Arc::clone(cause),
        }
    }

    /// The former effective declaration.
    pub const fn definition(&self) -> &CreateFunction {
        &self.definition
    }

    /// The DROP statement shared with the other deletions it caused.
    pub fn cause(&self) -> &DropFunction {
        &self.cause
    }
}

/// A dependent view removed by a DROP FUNCTION cascade.
#[derive(Debug, Clone)]
pub struct ViewTombstone {
    definition: ViewLike,
    cause: Arc<DropFunction>,
}

impl ViewTombstone {
    pub(crate) fn new(definition: ViewLike, cause: &Arc<DropFunction>) -> Self {
        Self {
            definition,
            cause: Arc::clone(cause),
        }
    }

    /// The former effective view declaration.
    pub const fn definition(&self) -> &ViewLike {
        &self.definition
    }

    /// The DROP FUNCTION statement responsible for removing the view.
    pub fn cause(&self) -> &DropFunction {
        &self.cause
    }
}

fn dependent_views(
    context: &impl ParseContext,
    targets: &[OwnershipIdentity],
) -> Result<Vec<FullName>, AnalysisError> {
    // TODO(#1185): Extend dependency traversal and removal to non-view objects.
    let names = context.relation_names();
    let mut removed = std::collections::HashSet::new();
    let mut views = Vec::new();
    loop {
        let previous_count = removed.len();
        for name in &names {
            if removed.contains(name) {
                continue;
            }
            let Some(view) = context.get_view_impl(name) else {
                continue;
            };
            let mut probe = ViewDependencyProbe {
                targets,
                removed: &removed,
                found: false,
            };
            view.query().visit(&mut probe)?;
            if probe.found {
                removed.insert(name.clone());
                views.push(name.clone());
            }
        }
        if previous_count == removed.len() {
            break;
        }
    }
    Ok(views)
}

struct ViewDependencyProbe<'a> {
    targets: &'a [OwnershipIdentity],
    removed: &'a std::collections::HashSet<FullName>,
    found: bool,
}

impl SemanticVisitor for ViewDependencyProbe<'_> {
    fn relation_identity(&mut self, name: &FullName) {
        self.found |= self.removed.contains(name);
    }

    fn schema_type(&mut self, reference: &TypeReference) -> Result<(), AnalysisError> {
        if let Some(name) = reference.full_name() {
            self.relation_identity(name);
        }
        Ok(())
    }

    fn expression(&mut self, expression: &SemScalarExpr) -> Result<(), AnalysisError> {
        if let Some(name) = expression.function_dependency()
            && let Some(call) = expression.as_function_call()
            && let Some(overload) = call.overload()
        {
            self.function_overload(name, overload);
        }
        Ok(())
    }

    fn function_overload(&mut self, name: &FullName, overload: &OverloadVariant) {
        let arguments = overload.declaration_arguments();
        self.found |= self.targets.iter().any(|identity| {
            identity.name() == name && identity.arguments() == Some(arguments.as_ref())
        });
    }
}
