use crate::{
    AstAndContextPair,
    ast::{DropBehavior, create_operator as ast},
    context::{ParseContext, SchemaFileContext},
    names::{FullName, OperatorName},
    sem::{
        AnalysisError, AnalysisProblem, OperatorFamily, OperatorIdentity, OwnershipIdentity,
        SemAst, TypeReference,
        operator_family::{MemberObject, MemberOwner},
        traversal::SemanticVisitor,
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

trait CascadeRemoval: std::fmt::Debug + Send + Sync {
    fn clone_box(&self) -> Box<dyn CascadeRemoval>;

    fn remove(&self, context: &mut SchemaFileContext) -> Result<(), AnalysisError>;
}

impl Clone for Box<dyn CascadeRemoval> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

#[derive(Clone, Debug)]
struct DependentView(FullName);

impl CascadeRemoval for DependentView {
    fn clone_box(&self) -> Box<dyn CascadeRemoval> {
        Box::new(self.clone())
    }

    fn remove(&self, context: &mut SchemaFileContext) -> Result<(), AnalysisError> {
        context.remove_operator_dependent_view(&self.0)
    }
}

#[derive(Clone, Debug)]
struct FamilyRemoval {
    family: OperatorFamily,
    classes: Vec<FullName>,
}

impl CascadeRemoval for FamilyRemoval {
    fn clone_box(&self) -> Box<dyn CascadeRemoval> {
        Box::new(self.clone())
    }

    fn remove(&self, context: &mut SchemaFileContext) -> Result<(), AnalysisError> {
        context.apply_operator_family_removal(&self.family, &self.classes);
        Ok(())
    }
}

impl FamilyRemoval {
    fn plan(family: &OperatorFamily, targets: &[DropOperatorTarget]) -> Option<Self> {
        let mut classes = Vec::new();
        let mut removed = Vec::new();
        for (slot, member) in &family.members {
            let MemberObject::Operator(name) = &member.object else {
                continue;
            };
            if matches_target(targets, name, Some(&slot.left), &slot.right) {
                removed.push(slot);
                if let MemberOwner::Class(class) = &member.owner {
                    classes.push(class.as_ref().clone());
                }
            }
        }
        if removed.is_empty() {
            return None;
        }
        classes.sort_by_cached_key(FullName::to_string);
        classes.dedup();
        let mut family = family.clone();
        family.members.retain(|slot, member| {
            !removed.contains(&slot)
                && !matches!(&member.owner, MemberOwner::Class(class) if classes.contains(class))
        });
        Some(Self { family, classes })
    }
}

fn matches_target(
    targets: &[DropOperatorTarget],
    name: &OperatorName,
    left: Option<&TypeReference>,
    right: &TypeReference,
) -> bool {
    targets.iter().any(|target| {
        matches!(target, DropOperatorTarget::Resolved(identity) if identity.matches(name, left, right))
    })
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
        let mut targets = Vec::with_capacity(ast.targets.len());
        let mut valid = true;
        for signature in &ast.targets {
            let target = Self::resolve_target(signature, ast.if_exists, context)?;
            valid &= !matches!(target, DropOperatorTarget::Rejected);
            targets.push(target);
        }
        let mut dependent_removals: Vec<Box<dyn CascadeRemoval>> = Vec::new();
        if valid {
            let views = dependent_views(context, &targets)?;
            if ast.behavior == DropBehavior::Cascade {
                dependent_removals.extend(
                    views
                        .into_iter()
                        .map(|name| Box::new(DependentView(name)) as Box<dyn CascadeRemoval>),
                );
            } else if let Some(name) = views.first() {
                Self::reject(context, format!("view {name} depends on the operator"))?;
                valid = false;
            }
        }
        if valid
            && targets
                .iter()
                .any(|target| matches!(target, DropOperatorTarget::Resolved(_)))
        {
            for key in context.operator_family_keys() {
                let Some(family) = context.operator_family(&key) else {
                    continue;
                };
                let Some(removal) = FamilyRemoval::plan(family, &targets) else {
                    continue;
                };
                if ast.behavior != DropBehavior::Cascade
                    && let Some(class) = removal.classes.first()
                {
                    Self::reject(
                        context,
                        format!("operator class {class} depends on the operator"),
                    )?;
                    valid = false;
                    break;
                }
                dependent_removals.push(Box::new(removal));
            }
        }
        Ok(Self {
            targets,
            behavior: ast.behavior,
            valid,
            dependent_removals,
        })
    }

    fn resolve_target(
        signature: &ast::OperatorSignature,
        if_exists: bool,
        context: &mut impl ParseContext,
    ) -> Result<DropOperatorTarget, AnalysisError> {
        let name = match OperatorName::from_ast_name(&signature.name) {
            Ok(name) => name,
            Err(problem) => {
                problem.report_problem_to(context)?;
                return Ok(DropOperatorTarget::Rejected);
            }
        };
        let [left, right] = signature
            .operands
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

fn dependent_views(
    context: &impl ParseContext,
    targets: &[DropOperatorTarget],
) -> Result<Vec<FullName>, AnalysisError> {
    let mut removed = std::collections::HashSet::new();
    let mut views = Vec::new();
    if !targets
        .iter()
        .any(|target| matches!(target, DropOperatorTarget::Resolved(_)))
    {
        return Ok(views);
    }
    let names = context.relation_names();
    loop {
        let previous_count = views.len();
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
        if previous_count == views.len() {
            return Ok(views);
        }
    }
}

struct ViewDependencyProbe<'a> {
    targets: &'a [DropOperatorTarget],
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

    fn operator_overload(
        &mut self,
        name: &OperatorName,
        left: Option<&TypeReference>,
        right: &TypeReference,
    ) {
        self.found |= matches_target(self.targets, name, left, right);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{
        context::{OperatorInfoRead, ParseContext, ViewInfoRead},
        names::{FullName, OperatorName, SchemaName},
        sem::SemAst,
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
        Ok(())
    }

    #[test]
    fn rejects_group_then_removes_one_overload_and_dependent_view() -> testresult::TestResult {
        let mut inventory = TestParseContext::default();
        inventory.set_get_search_path_result(&[SchemaName::from("public")]);
        for name in ["int4", "bool"] {
            inventory.setup_type(super::super::builtin(name));
        }
        let base = inventory.freeze();
        let mut context = schema_layer(Path::new("operators.sql"), &base);
        for sql in [
            "CREATE FUNCTION op(integer, integer) RETURNS boolean LANGUAGE sql AS 'SELECT true'",
            "CREATE FUNCTION op(integer) RETURNS boolean LANGUAGE sql AS 'SELECT true'",
            "CREATE OPERATOR %% (LEFTARG=integer, RIGHTARG=integer, FUNCTION=op)",
            "CREATE OPERATOR %% (RIGHTARG=integer, FUNCTION=op)",
            "CREATE VIEW v AS SELECT 1 %% 1 AS n",
        ] {
            context = apply_ownership_sql(context, sql)?.into_context();
        }
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let parent = context.freeze();
        let operator = OperatorName::from_parts(Some("public"), "%%");
        let view = FullName::with_schema("public", "v");
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
        let cascaded = apply_ownership_sql(
            schema_layer(Path::new("cascade.sql"), &parent),
            "DROP OPERATOR public.%% (integer, integer) CASCADE",
        )?;
        assert!(matches!(cascaded.ast(), SemAst::DropOperator(drop) if drop.applicable()));
        let child = cascaded.into_context().freeze();
        assert!(child.get_operator_op(&operator, true, true).is_none());
        assert!(child.get_operator_op(&operator, false, true).is_some());
        assert!(child.get_view_impl(&view).is_none());
        assert!(parent.get_view_impl(&view).is_some());
        assert!(parent.get_operator_op(&operator, true, true).is_some());
        Ok(())
    }
}
