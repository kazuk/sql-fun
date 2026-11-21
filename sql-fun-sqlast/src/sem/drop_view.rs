use crate::{
    ast::{self, ObjectKind},
    context::ParseContext,
    names::{FullName, TableName, ViewName},
    sem::{
        AnalysisError, AnalysisProblem, DropDependencyBehavior, Kind, OwnershipIdentity,
        SourcePathWithSpan, problem::ViewProblem, search_path::expand_relation_search_path,
    },
};

/// Resolution of one requested DROP VIEW target.
#[derive(Clone, Debug)]
pub enum DropViewLookup {
    /// An ordinary view at its effective catalog identity.
    Resolved(FullName),
    /// No relation is visible under the requested name.
    Missing,
    /// A visible relation has a different kind.
    WrongKind(FullName, Kind),
    /// The explicit database is outside the current catalog.
    CrossDatabase,
}

/// The written target and its completed catalog lookup.
#[derive(Clone, Debug)]
pub struct DropViewTarget {
    requested: FullName,
    lookup: DropViewLookup,
}

impl DropViewTarget {
    /// Name and qualification supplied by the statement.
    pub const fn requested(&self) -> &FullName {
        &self.requested
    }

    /// Result of resolving the requested name.
    pub const fn lookup(&self) -> &DropViewLookup {
        &self.lookup
    }
}

/// PostgreSQL DROP VIEW and the outcome of its catalog validation.
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct DropView {
    targets: Vec<DropViewTarget>,
    source: Option<SourcePathWithSpan>,
    #[getter(copy)]
    if_exists: bool,
    #[getter(copy)]
    behavior: DropDependencyBehavior,
    removals: Vec<FullName>,
    dependent_views: Vec<FullName>,
    blocking_members: Vec<FullName>,
    #[getter(copy)]
    applicable: bool,
}

impl DropView {
    pub(crate) fn analyze(
        ast: &ast::DropView,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut statement = Self {
            targets: Vec::new(),
            source: context.get_source_path_with_span().cloned(),
            if_exists: ast.if_exists(),
            behavior: DropDependencyBehavior::from_ast(&ast.behavior()),
            removals: Vec::new(),
            dependent_views: Vec::new(),
            blocking_members: Vec::new(),
            applicable: true,
        };
        for name in ast.targets() {
            statement.resolve_target(FullName::from_ast_name(name)?, context)?;
        }
        if statement.applicable && !statement.removals.is_empty() {
            statement.validate_dependencies(context)?;
        }
        Ok(statement)
    }

    fn resolve_target(
        &mut self,
        request: FullName,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        let outcome = Self::lookup(&request, context);
        let problem = match &outcome {
            DropViewLookup::Resolved(name) => {
                if !self.removals.contains(name) {
                    self.removals.push(name.clone());
                }
                None
            }
            DropViewLookup::Missing if self.if_exists => None,
            DropViewLookup::Missing => Some(AnalysisProblem::view_not_found(
                &ViewName::from_full_name(&request),
            )),
            DropViewLookup::WrongKind(name, kind) => Some(AnalysisProblem::view(
                &ViewProblem::drop_wrong_kind(name, *kind),
            )),
            DropViewLookup::CrossDatabase => Some(AnalysisProblem::view(
                &ViewProblem::drop_cross_database(&request),
            )),
        };
        self.targets.push(DropViewTarget {
            requested: request,
            lookup: outcome,
        });
        if let Some(problem) = problem {
            self.applicable = false;
            context.report_analysis_problem(problem)?;
        }
        Ok(())
    }

    fn lookup(request: &FullName, context: &impl ParseContext) -> DropViewLookup {
        if let Some(database) = request.catalog()
            && context.metadata().database_name() != Some(database.as_str())
        {
            return DropViewLookup::CrossDatabase;
        }
        let name = FullName::new(&None, &request.schema().cloned(), request.name());
        match expand_relation_search_path(context, &name, |key| {
            context.object_kind(key).map(|kind| (key.clone(), kind))
        }) {
            Some((name, Kind::View)) => DropViewLookup::Resolved(name),
            Some((name, kind)) => DropViewLookup::WrongKind(name, kind),
            None => DropViewLookup::Missing,
        }
    }

    fn validate_dependencies(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        let mut visited = self.removals.clone();
        let mut default_owners = Vec::new();
        let mut index = 0;
        while let Some(name) = visited.get(index) {
            let dependents = context.view_dependents(name);
            default_owners.extend(dependents.defaults);
            for view in dependents.queries {
                if !visited.contains(&view) {
                    visited.push(view);
                }
            }
            index += 1;
        }
        self.dependent_views = visited[self.removals.len()..].to_vec();
        if !self.behavior.is_cascade() {
            for owner in default_owners
                .iter()
                .filter(|owner| !visited.contains(owner))
            {
                if !self.dependent_views.contains(owner) {
                    self.dependent_views.push(owner.clone());
                }
            }
        }
        if self.behavior.is_cascade() {
            self.removals.extend(self.dependent_views.iter().cloned());
        } else {
            for name in &self.dependent_views {
                self.applicable = false;
                context.report_analysis_problem(AnalysisProblem::view(
                    &ViewProblem::drop_dependency(name),
                ))?;
            }
        }
        for name in &self.removals {
            let kind = match context.object_kind(name) {
                Some(Kind::MaterializedView) => ObjectKind::MATERIALIZED_VIEW,
                _ => ObjectKind::VIEW,
            };
            let member = OwnershipIdentity::named(kind, name);
            let Some(extension) = context.extension_membership(&member) else {
                continue;
            };
            if context.active_extension() == Some(extension) {
                continue;
            }
            if self.dependent_views.contains(name) {
                if self.applicable {
                    todo!("DROP VIEW cascade through an extension-owned dependent (#1197)");
                }
                continue;
            }
            self.applicable = false;
            self.blocking_members.push(name.clone());
            context.report_analysis_problem(AnalysisProblem::view(
                &ViewProblem::drop_extension_member(name),
            ))?;
        }
        if self.applicable
            && self.behavior.is_cascade()
            && default_owners
                .iter()
                .any(|owner| !self.removals.contains(owner))
        {
            todo!("DROP VIEW CASCADE removal of a surviving view's column default (#1197)");
        }
        if self.applicable {
            for name in &self.removals {
                if context.object_kind(name) == Some(Kind::MaterializedView)
                    && !context
                        .indexes_for_table(&TableName::from(name.clone()))
                        .is_empty()
                {
                    todo!(
                        "DROP VIEW CASCADE removal of an index on a dependent materialized view (#1197)"
                    );
                }
            }
        }
        Ok(())
    }
}
