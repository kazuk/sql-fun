use crate::{
    AstAndContextPair,
    ast::{DropBehavior, create_operator as ast},
    context::{BaseParseContext, ParseContext, SchemaFileContext},
    names::{FullName, OperatorName},
    sem::{
        AnalysisError, AnalysisProblem, OperatorFamilyKey, OperatorIdentity, OwnershipIdentity,
        SemAst,
    },
};

/// The outcome for one written DROP OPERATOR target.
#[derive(Clone, Debug)]
pub enum DropOperatorTarget {
    /// An exact operator signature was found.
    Resolved(Box<OperatorIdentity>),
    /// IF EXISTS permitted a missing operator or operand type.
    Skipped,
    /// Resolution failed.
    Rejected,
}

/// An analyzed PostgreSQL DROP OPERATOR statement.
#[derive(Clone, Debug)]
pub struct DropOperator {
    targets: Vec<DropOperatorTarget>,
    behavior: DropBehavior,
    valid: bool,
    dependent_removals: Vec<Box<dyn CascadeRemoval>>,
}

/// Identity of a catalog removal registration.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CascadeTarget {
    /// A view in the relation namespace.
    View(FullName),
    /// The members contributed by one overload to an operator family.
    OperatorFamily(OperatorFamilyKey, Box<OperatorIdentity>),
}

/// Catalog action registered for an object subject to cascading deletion.
pub trait CascadeRemoval: std::fmt::Debug + Send + Sync {
    /// Identity used to replace and deduplicate this registration.
    fn target(&self) -> CascadeTarget;

    /// Dependency diagnostic for RESTRICT, or absence for automatic removal.
    fn restrict_problem(&self) -> Option<String>;

    /// Clone the registered action.
    fn clone_box(&self) -> Box<dyn CascadeRemoval>;

    /// Removals registered through this object's dependents.
    fn dependents(&self, _catalog: &dyn BaseParseContext) -> Vec<Box<dyn CascadeRemoval>> {
        Vec::new()
    }

    /// Apply the removal to the effective catalog.
    ///
    /// # Errors
    /// Returns an error if the catalog cannot apply the registered removal.
    fn remove(&self, context: &mut SchemaFileContext) -> Result<(), AnalysisError>;
}

impl Clone for Box<dyn CascadeRemoval> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

impl DropOperator {
    /// Resolution outcomes in statement order.
    pub fn targets(&self) -> &[DropOperatorTarget] {
        &self.targets
    }

    /// Whether the statement requested CASCADE.
    pub fn cascade(&self) -> bool {
        self.behavior == DropBehavior::Cascade
    }

    /// Whether PostgreSQL accepts the analyzed statement.
    pub const fn valid(&self) -> bool {
        self.valid
    }

    /// Whether the statement can update the catalog.
    pub fn applicable(&self) -> bool {
        self.valid
            && self
                .targets
                .iter()
                .any(|target| matches!(target, DropOperatorTarget::Resolved(_)))
    }

    pub(crate) fn analyze_and_apply<C: ParseContext>(
        ast: &ast::DropOperator,
        mut context: C,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        let statement = Self::analyze(ast, &mut context)?;
        let context = context.apply_drop_operator(&statement)?;
        Ok(AstAndContextPair::new(
            SemAst::DropOperator(Box::new(statement)),
            context,
        ))
    }

    pub(crate) fn analyze(
        ast: &ast::DropOperator,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut targets = Vec::with_capacity(ast.targets().len());
        let mut valid = true;
        for signature in ast.targets() {
            let target = Self::resolve_target(signature, ast.if_exists(), context)?;
            valid &= !matches!(target, DropOperatorTarget::Rejected);
            targets.push(target);
        }
        let mut dependent_removals = Vec::new();
        if valid {
            dependent_removals = registered_operator_removals(context, &targets);
            if ast.behavior() != DropBehavior::Cascade
                && let Some(problem) = dependent_removals
                    .iter()
                    .find_map(|action| action.restrict_problem())
            {
                Self::reject(context, problem)?;
                valid = false;
            }
        }
        Ok(Self {
            targets,
            behavior: ast.behavior(),
            valid,
            dependent_removals,
        })
    }

    fn resolve_target(
        signature: &ast::OperatorSignature,
        if_exists: bool,
        context: &mut impl ParseContext,
    ) -> Result<DropOperatorTarget, AnalysisError> {
        let name = match OperatorName::from_ast_name(signature.name()) {
            Ok(name) => name,
            Err(problem) => {
                problem.report_problem_to(context)?;
                return Ok(DropOperatorTarget::Rejected);
            }
        };
        let [left, right] = signature
            .operands()
            .each_ref()
            .map(|arg| arg.type_name().as_ref());
        let left_type = left
            .map(|ty| OwnershipIdentity::argument_type_from_ast(ty, context))
            .transpose()?
            .flatten();
        let right_type = right
            .map(|ty| OwnershipIdentity::argument_type_from_ast(ty, context))
            .transpose()?
            .flatten();
        if let Some(right_type) = right_type
            && (left.is_none() || left_type.is_some())
            && let Some((identity, _)) =
                OperatorIdentity::resolve(&name, left_type.as_ref(), &right_type, context)
        {
            if context.operator_is_system_required(&identity) {
                return Self::reject(
                    context,
                    format!("operator {name} is required by the database system"),
                );
            }
            if let Some(member) = identity.extension_member()
                && let Some(extension) = context.extension_membership(&member)
                && context.active_extension() != Some(extension)
            {
                let message = format!("extension {extension} requires operator {member}");
                return Self::reject(context, message);
            }
            return Ok(DropOperatorTarget::Resolved(Box::new(identity)));
        }
        if if_exists {
            return Ok(DropOperatorTarget::Skipped);
        }
        Self::reject(
            context,
            format!(
                "operator {} with the specified operand types does not exist",
                name.to_name_string()
            ),
        )
    }

    fn reject(
        context: &mut impl ParseContext,
        message: String,
    ) -> Result<DropOperatorTarget, AnalysisError> {
        let span = context.get_span().map(Into::into);
        context
            .report_analysis_problem(AnalysisProblem::InvalidOperatorDeclaration(message, span))?;
        Ok(DropOperatorTarget::Rejected)
    }

    pub(crate) fn apply(&self, context: &mut SchemaFileContext) -> Result<(), AnalysisError> {
        if !self.applicable() {
            return Ok(());
        }
        for target in &self.dependent_removals {
            target.remove(context)?;
        }
        let mut removed = std::collections::HashSet::new();
        for target in &self.targets {
            if let DropOperatorTarget::Resolved(identity) = target
                && removed.insert(identity)
            {
                context.remove_operator(identity)?;
            }
        }
        Ok(())
    }
}

fn registered_operator_removals(
    context: &impl ParseContext,
    targets: &[DropOperatorTarget],
) -> Vec<Box<dyn CascadeRemoval>> {
    let mut pending = std::collections::VecDeque::new();
    for target in targets {
        if let DropOperatorTarget::Resolved(identity) = target
            && let Some(definition) =
                context.get_operator_op(identity.name(), identity.binary(), true)
            && let Some(dependents) = identity.dependents(definition)
        {
            pending.extend(dependents.entries().iter().cloned());
        }
    }
    let mut visited = std::collections::HashSet::new();
    let mut removals = Vec::new();
    while let Some(removal) = pending.pop_front() {
        if visited.insert(removal.target()) {
            pending.extend(removal.dependents(context));
            removals.push(removal);
        }
    }
    removals
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{
        context::{OperatorInfoRead, ParseContext, ViewInfoRead},
        names::{FullName, OperatorName, SchemaName},
        sem::{SemAst, create_operator::builtin},
        test_helpers::{TestParseContext, ownership::apply_ownership_sql, schema_layer},
    };

    #[test]
    fn class_dependency_blocks_restrict_and_cascades_from_a_child_layer() -> testresult::TestResult
    {
        use crate::{
            context::{ConflictInfoRead, OperatorFamilyInfoRead},
            sem::{OperatorFamilyKey, PgBuiltInType},
            test_helpers::{conflict_masters::Masters, schema_creation},
        };

        let mut builtin = Masters::new().context();
        builtin.setup_type(PgBuiltInType::int4());
        builtin.setup_type(PgBuiltInType::bool());
        let mut context = schema_creation::child(builtin.freeze());
        context.set_search_path(&[SchemaName::from("public")]);
        let parent = schema_creation::apply(
            context,
            "CREATE ACCESS METHOD btree TYPE INDEX HANDLER handler;
             CREATE FUNCTION cmp(integer, integer) RETURNS boolean LANGUAGE sql AS 'SELECT true';
             CREATE OPERATOR %% (LEFTARG = integer, RIGHTARG = integer, FUNCTION = cmp);
             CREATE OPERATOR ## (LEFTARG = integer, RIGHTARG = integer, FUNCTION = cmp);
             CREATE OPERATOR FAMILY loose_drop_ops USING btree;
             ALTER OPERATOR FAMILY loose_drop_ops USING btree ADD OPERATOR 3 ## (integer, integer);
             CREATE OPERATOR CLASS drop_ops FOR TYPE integer USING btree AS OPERATOR 3 %% (integer, integer);",
        )?;
        assert!(parent.problems().is_empty(), "{:?}", parent.problems());
        let parent = parent.freeze();
        let class = FullName::with_schema("public", "drop_ops");
        let family = OperatorFamilyKey::new(class.clone(), "btree");
        let restricted = apply_ownership_sql(
            schema_layer(Path::new("restrict.sql"), &parent),
            "DROP OPERATOR %% (integer, integer) RESTRICT",
        )?;
        assert!(matches!(restricted.ast(), SemAst::DropOperator(drop) if !drop.valid()));
        let dropped = apply_ownership_sql(
            schema_layer(Path::new("cascade.sql"), &parent),
            "DROP OPERATOR %% (integer, integer) CASCADE",
        )?;
        assert!(matches!(dropped.ast(), SemAst::DropOperator(drop) if drop.applicable()));
        let child = dropped.into_context().freeze();
        assert!(child.operator_class_definition(&class, "btree").is_none());
        assert!(child.operator_family(&family).unwrap().members.is_empty());
        assert!(parent.operator_class_definition(&class, "btree").is_some());
        assert_eq!(parent.operator_family(&family).unwrap().members.len(), 1);

        let loose =
            OperatorFamilyKey::new(FullName::with_schema("public", "loose_drop_ops"), "btree");
        let automatic = apply_ownership_sql(
            schema_layer(Path::new("automatic.sql"), &parent),
            "DROP OPERATOR ## (integer, integer) RESTRICT",
        )?;
        assert!(matches!(automatic.ast(), SemAst::DropOperator(drop) if drop.applicable()));
        assert!(
            automatic
                .context()
                .operator_family(&loose)
                .unwrap()
                .members
                .is_empty()
        );
        assert_eq!(parent.operator_family(&loose).unwrap().members.len(), 1);
        Ok(())
    }

    #[test]
    fn rejects_group_then_removes_one_overload_and_dependent_view() -> testresult::TestResult {
        let mut inventory = TestParseContext::default();
        inventory.set_get_search_path_result(&[SchemaName::from("public")]);
        for name in ["int4", "bool"] {
            inventory.setup_type(builtin(name));
        }
        let base = inventory.freeze();
        let mut context = schema_layer(Path::new("operators.sql"), &base);
        for sql in [
            "CREATE FUNCTION op(integer, integer) RETURNS boolean LANGUAGE sql AS 'SELECT true'",
            "CREATE FUNCTION op(integer) RETURNS boolean LANGUAGE sql AS 'SELECT true'",
            "CREATE OPERATOR %% (LEFTARG=integer, RIGHTARG=integer, FUNCTION=op)",
            "CREATE OPERATOR %% (RIGHTARG=integer, FUNCTION=op)",
            "CREATE VIEW v AS SELECT 1 %% 1 AS n",
            "CREATE VIEW child_v AS SELECT n FROM v",
            "CREATE VIEW any_v AS SELECT 1 OPERATOR(public.%%) ANY (ARRAY[2]) AS n",
            "CREATE VIEW all_v AS SELECT 1 OPERATOR(public.%%) ALL (ARRAY[2]) AS n",
            "CREATE VIEW replaced_v AS SELECT 1 %% 1 AS n",
        ] {
            context = apply_ownership_sql(context, sql)?.into_context();
        }
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let parent = context.freeze();
        let operator = OperatorName::from_parts(Some("public"), "%%");
        let view = FullName::with_schema("public", "v");
        let dependent = FullName::with_schema("public", "child_v");
        let replaced = FullName::with_schema("public", "replaced_v");
        let restricted = apply_ownership_sql(
            schema_layer(Path::new("restrict.sql"), &parent),
            "DROP OPERATOR public.%% (NONE, integer), public.%% (integer, integer)",
        )?;
        assert!(matches!(restricted.ast(), SemAst::DropOperator(drop) if !drop.valid()));
        assert!(
            restricted
                .context()
                .get_operator_op(&operator, false, true)
                .is_some()
        );
        assert!(
            restricted
                .context()
                .get_operator_op(&operator, true, true)
                .is_some()
        );
        let cascade_layer = apply_ownership_sql(
            schema_layer(Path::new("cascade.sql"), &parent),
            "CREATE OR REPLACE VIEW replaced_v AS SELECT true AS n",
        )?
        .into_context();
        let cascaded = apply_ownership_sql(
            cascade_layer,
            "DROP OPERATOR public.%% (integer, integer) CASCADE",
        )?;
        assert!(matches!(cascaded.ast(), SemAst::DropOperator(drop) if drop.applicable()));
        let child = cascaded.into_context().freeze();
        assert!(child.get_operator_op(&operator, true, true).is_none());
        assert!(child.get_operator_op(&operator, false, true).is_some());
        assert!(child.get_view_impl(&view).is_none());
        assert!(child.get_view_impl(&dependent).is_none());
        for name in ["any_v", "all_v"] {
            let view = FullName::with_schema("public", name);
            assert!(parent.get_view_impl(&view).is_some());
            assert!(child.get_view_impl(&view).is_none());
        }
        assert!(child.get_view_impl(&replaced).is_some());
        assert!(parent.get_view_impl(&view).is_some());
        assert!(parent.get_view_impl(&dependent).is_some());
        assert!(parent.get_operator_op(&operator, true, true).is_some());
        Ok(())
    }
}
