mod implementation {
    mod analysis;

    use crate::{
        StringSpan,
        ast::Call,
        context::BaseParseContext,
        names::{FullName, TableName},
        sem::{
            AnalysisError, FunctionModifiers, FunctionOverloadCollection, FunctionParam,
            NamedArgExpr, OverloadVariant, ParseContext, QueryNameResolution, SemScalarExpr,
            scalar_expr::SemScalarExprNode,
            traversal::{SemanticVisitor, SemanticVisitorMut},
            type_system::ArgumentBindingCollection,
        },
    };
    use ::sql_fun_core::IVec;

    /// function call expression
    #[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct FuncCallExpr {
        func_name: FullName,
        name_resolution: QueryNameResolution,
        #[serde(default)]
        candidates: Option<Vec<OverloadVariant>>,
        overload: Option<OverloadVariant>,
        #[serde(default)]
        signature_deferred: bool,
        #[serde(deserialize_with = "deserialize_arguments")]
        args: IVec<FuncCallArgs>,
        #[serde(default)]
        modifiers: Box<FunctionModifiers>,
    }

    fn deserialize_arguments<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<IVec<FuncCallArgs>, D::Error> {
        let arguments = <Vec<FuncCallArgs> as serde::Deserialize>::deserialize(deserializer)?;
        Ok(arguments
            .into_iter()
            .enumerate()
            .map(|(position, argument)| argument.at_position(position))
            .collect::<Vec<_>>()
            .into())
    }

    /// A supplied argument, its call position, and any independently established formal association.
    #[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct FuncCallArgs {
        arg_expr: SemScalarExpr,
        arg_definition: Option<FunctionParam>,
        #[serde(default)]
        position: usize,
    }

    /// Function identity and SQL modifiers for a parameter-dependent call.
    /// Operands are owned by the enclosing constrained expression.
    #[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct FunctionBinding {
        name: FullName,
        resolution: QueryNameResolution,
        modifiers: Box<FunctionModifiers>,
        #[serde(default)]
        overload: Option<OverloadVariant>,
        #[serde(default)]
        candidates: Option<Vec<OverloadVariant>>,
        #[serde(default)]
        argument_definitions: Vec<Option<FunctionParam>>,
    }
    impl FunctionBinding {
        /// Selected signature, when the supplied types establish one.
        pub const fn overload(&self) -> Option<&OverloadVariant> {
            self.overload.as_ref()
        }
        /// Formal association at a position in the supplied argument order.
        pub fn argument_definition(&self, position: usize) -> Option<&FunctionParam> {
            if let Some(definition) = self.argument_definitions.get(position) {
                definition.as_ref()
            } else {
                None
            }
        }
        pub(super) fn retain_bound_arguments(
            &mut self,
            overload: &OverloadVariant,
            args: &IVec<FuncCallArgs>,
        ) {
            self.overload = Some(overload.clone());
            self.argument_definitions = args
                .iter()
                .map(|argument| argument.arg_definition().clone())
                .collect();
        }
        /// Resolved function identity (or original name after a failed lookup).
        pub const fn name(&self) -> &FullName {
            &self.name
        }
        /// Completed name lookup, independent of overload selection.
        pub const fn resolution(&self) -> &QueryNameResolution {
            &self.resolution
        }
        /// SQL modifiers owned by this call.
        pub fn modifiers(&self) -> &FunctionModifiers {
            &self.modifiers
        }
        pub(crate) fn walk<V: SemanticVisitorMut>(
            &mut self,
            visitor: &mut V,
        ) -> Result<(), AnalysisError> {
            self.modifiers.walk(visitor)
        }
        pub(crate) fn visit<V: SemanticVisitor>(
            &self,
            visitor: &mut V,
        ) -> Result<(), AnalysisError> {
            if let Some(name) = self.resolution.function_identity() {
                visitor.schema_function(name)?;
            }
            self.modifiers.visit(visitor)
        }
    }
    impl FuncCallExpr {
        pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
            self.name_resolution.rename_relation(old, new);
            if let Some(overload) = &mut self.overload {
                overload.rename_relation(old, new);
            }
            if let Some(candidates) = &mut self.candidates {
                for candidate in candidates {
                    candidate.rename_relation(old, new);
                }
            }
            let mut arguments = self.args.to_vec();
            for argument in &mut arguments {
                if let Some(definition) = &mut argument.arg_definition {
                    definition.rename_relation(old, new);
                }
            }
            self.args = arguments.into();
        }
        pub(crate) fn canonicalize_grouping_key(&mut self) {
            if let Some(name) = self.name_resolution.function_identity() {
                self.func_name = name.clone();
            }
            self.candidates = None;
        }

        pub(crate) fn binding_snapshot(&self) -> FunctionBinding {
            FunctionBinding {
                name: self.func_name.clone(),
                resolution: self.name_resolution.clone(),
                modifiers: self.modifiers.clone(),
                overload: self.overload.clone(),
                candidates: self.candidates.clone(),
                argument_definitions: self
                    .args
                    .iter()
                    .map(|argument| argument.arg_definition.clone())
                    .collect(),
            }
        }
    }
    impl FuncCallExpr {
        /// create a instance
        #[must_use]
        pub fn new(
            func_name: &FullName,
            overload: &Option<OverloadVariant>,
            args: &IVec<FuncCallArgs>,
        ) -> Self {
            let mut positioned = Vec::new();
            for argument in args {
                positioned.push(argument.at_position(positioned.len()));
            }
            Self {
                func_name: func_name.clone(),
                candidates: None,
                name_resolution: if overload.is_some() {
                    QueryNameResolution::function(func_name)
                } else {
                    QueryNameResolution::not_found()
                },
                overload: overload.clone(),
                signature_deferred: false,
                args: positioned.into(),
                modifiers: Box::default(),
            }
        }
    }

    #[cfg(test)]
    mod test_supplied_arguments {
        use crate::{
            names::FullName,
            sem::{
                FunctionParam, PgBuiltInType, ScalarConstExpr, SemScalarExpr,
                scalar_expr::func_call::implementation::{FuncCallArgs, FuncCallExpr},
            },
        };

        #[test]
        fn owning_call_assigns_positions_without_losing_known_associations() {
            let value = SemScalarExpr::literal(&ScalarConstExpr::new_integer(1));
            let formal = FunctionParam::new_input_param(
                &Some("known".to_owned()),
                &Some(PgBuiltInType::int4()),
                &None,
            );
            let named = SemScalarExpr::named_argument("known", &value, &None);
            let arguments = vec![
                FuncCallArgs::new(&value, &None),
                FuncCallArgs::new(&named, &Some(formal.clone())),
            ]
            .into();
            let call = FuncCallExpr::new(&FullName::from_local_name("f"), &None, &arguments);
            assert_eq!(call.args()[0].position(), 0);
            assert_eq!(call.args()[0].supplied_name(), None);
            assert_eq!(call.args()[1].position(), 1);
            assert_eq!(call.args()[1].supplied_name(), Some("known"));
            assert_eq!(call.args()[1].arg_definition(), &Some(formal));
            assert_eq!(call.args()[1].value(), &value);
            let restored: FuncCallExpr =
                ::serde_json::from_str(&::serde_json::to_string(&call).unwrap()).unwrap();
            assert_eq!(restored, call);
            let mut legacy = ::serde_json::to_value(&call).unwrap();
            for argument in legacy["args"].as_array_mut().unwrap() {
                argument.as_object_mut().unwrap().remove("position");
            }
            let restored: FuncCallExpr = ::serde_json::from_value(legacy).unwrap();
            assert_eq!(restored, call);
        }
    }

    impl FuncCallExpr {
        pub(super) fn retain_signature(
            &mut self,
            overload: &OverloadVariant,
            args: &IVec<FuncCallArgs>,
        ) {
            self.overload = Some(overload.clone());
            self.args = args
                .iter()
                .enumerate()
                .map(|(position, argument)| argument.at_position(position))
                .collect::<Vec<_>>()
                .into();
            self.signature_deferred = false;
        }
        /// Whether signature binding is explicitly deferred, independently of name lookup.
        pub const fn signature_is_deferred(&self) -> bool {
            self.signature_deferred
        }
        pub(crate) fn set_candidates(&mut self, candidates: Option<Vec<OverloadVariant>>) {
            self.candidates = candidates;
        }
        pub(crate) fn returns_set(&self, context: &impl BaseParseContext) -> Option<bool> {
            let name = self.name_resolution().function_identity()?;
            if let Some(overload) = self.overload() {
                Some(context.function_returns_set(name, overload))
            } else {
                self.candidates()?.returns_set(context, name)
            }
        }
        pub(crate) fn candidates(&self) -> Option<FunctionOverloadCollection> {
            self.candidates
                .as_deref()
                .map(FunctionOverloadCollection::new)
        }
        pub(crate) fn set_name_resolution(&mut self, outcome: QueryNameResolution) {
            self.name_resolution = outcome;
        }
        /// Completed function-name lookup, independent of overload selection.
        pub const fn name_resolution(&self) -> &QueryNameResolution {
            &self.name_resolution
        }
        pub(crate) fn set_modifiers(&mut self, modifiers: FunctionModifiers) {
            *self.modifiers = modifiers;
        }
        /// SQL modifiers retained alongside the resolved function binding.
        pub fn modifiers(&self) -> &FunctionModifiers {
            &self.modifiers
        }
    }
    impl FuncCallExpr {
        pub(crate) const fn name(&self) -> &FullName {
            &self.func_name
        }

        /// Selected overload, independently of name lookup.
        #[must_use]
        pub const fn overload(&self) -> &Option<OverloadVariant> {
            &self.overload
        }
    }

    impl FuncCallExpr {
        pub(crate) fn argument_values(&self) -> IVec<SemScalarExpr> {
            self.args
                .iter()
                .map(|argument| argument.value().clone())
                .collect::<Vec<_>>()
                .into()
        }
        /// argument expression and definitions
        #[must_use]
        pub const fn args(&self) -> &IVec<FuncCallArgs> {
            &self.args
        }
    }

    impl FuncCallArgs {
        /// Preserve an expression and an already known formal association. The owning call assigns its position.
        pub fn new(arg_expr: &SemScalarExpr, arg_definition: &Option<FunctionParam>) -> Self {
            Self {
                arg_expr: arg_expr.clone(),
                arg_definition: arg_definition.clone(),
                position: 0,
            }
        }
    }

    impl FuncCallArgs {
        fn at_position(&self, position: usize) -> Self {
            let mut arg = self.clone();
            arg.position = position;
            arg
        }
        /// Zero-based position in the supplied call, independent of a formal index or `$n`.
        pub const fn position(&self) -> usize {
            self.position
        }
        /// Optional supplied name. `None` identifies a positional argument.
        pub fn supplied_name(&self) -> Option<&str> {
            self.arg_expr.as_named_argument().map(NamedArgExpr::name)
        }
        /// The value inside a named wrapper, or the positional expression itself.
        pub fn value(&self) -> &SemScalarExpr {
            self.arg_expr
                .as_named_argument()
                .map_or(&self.arg_expr, NamedArgExpr::value)
        }
        /// Source span of the supplied argument name, when reliably available.
        pub const fn source_span(&self) -> Option<&StringSpan> {
            if let Some(named) = self.arg_expr.as_named_argument() {
                named.source_span()
            } else {
                None
            }
        }
        /// Supplied expression, including its named wrapper when present.
        pub const fn arg_expr(&self) -> &SemScalarExpr {
            &self.arg_expr
        }
    }

    impl FuncCallArgs {
        /// Already established formal association; `None` does not imply an invalid argument.
        pub const fn arg_definition(&self) -> &Option<FunctionParam> {
            &self.arg_definition
        }
    }
    impl FuncCallExpr {
        pub(crate) fn walk<V: SemanticVisitorMut>(
            &mut self,
            visitor: &mut V,
        ) -> Result<(), AnalysisError> {
            let mut args = self.args.to_vec();
            for arg in &mut args {
                arg.arg_expr.walk(visitor)?;
            }
            self.args = args.into();
            self.modifiers.walk(visitor)?;
            Ok(())
        }
        pub(crate) fn visit<V: SemanticVisitor>(
            &self,
            visitor: &mut V,
        ) -> Result<(), AnalysisError> {
            if let Some(name) = self.name_resolution.function_identity() {
                visitor.schema_function(name)?;
            }
            for arg in self.args.iter() {
                arg.arg_expr.visit(visitor)?;
            }
            self.modifiers.visit(visitor)?;
            Ok(())
        }
        pub(crate) fn inference_call(&self) -> (&FullName, Vec<SemScalarExpr>) {
            (
                &self.func_name,
                self.args.iter().map(|arg| arg.arg_expr.clone()).collect(),
            )
        }
    }

    impl FunctionBinding {
        pub(crate) fn invocation(&self, arguments: &[SemScalarExpr]) -> FuncCallExpr {
            let args = arguments
                .iter()
                .enumerate()
                .map(|(position, argument)| {
                    FuncCallArgs::new(argument, &self.argument_definition(position).cloned())
                })
                .collect::<Vec<_>>();
            let mut call = FuncCallExpr::new(&self.name, &self.overload, &args.into());
            call.set_name_resolution(self.resolution.clone());
            call.set_candidates(self.candidates.clone());
            call.set_modifiers(*self.modifiers.clone());
            call
        }
        pub(crate) fn finalize(
            &mut self,
            context: &mut impl ParseContext,
            arguments: &mut Vec<SemScalarExpr>,
            not_null: &mut Option<bool>,
        ) -> Result<(), AnalysisError> {
            let types = ArgumentBindingCollection::from_expr_list(arguments)
                .with_explicit_variadic(self.modifiers.variadic());
            if !types.has_call_notation() {
                return Ok(());
            }
            if let Some(candidates) = &self.candidates
                && !arguments.iter().any(SemScalarExpr::requires_argument_type)
                && let Some((overload, common_inputs)) = FunctionOverloadCollection::new(candidates)
                    .resolve_in_context_with_common_inputs(context, &types)
            {
                let args = FuncCallExpr::cast_common_call_operands(
                    context,
                    &self.name,
                    &arguments.clone().into(),
                    &overload,
                    self.modifiers.variadic(),
                    &common_inputs,
                )?;
                self.retain_bound_arguments(&overload, &args);
                *arguments = args
                    .iter()
                    .map(|argument| argument.arg_expr().clone())
                    .collect();
            }
            *not_null = self.invocation(arguments).is_not_null();
            Ok(())
        }
    }

    impl FuncCallExpr {
        pub(crate) fn from_ast_supplied_call(
            ast: &Call,
            context: &mut impl ParseContext,
            arguments: &[SemScalarExpr],
        ) -> Result<Self, AnalysisError> {
            Self::resolve_supplied(
                context,
                &FullName::from_ast_name(ast.name())?,
                arguments,
                ast.argument_locations(),
                ast.location(),
                ast.variadic(),
                ast.over().is_some(),
            )
        }
    }

    #[cfg(test)]
    mod test_aggregate_star_does_not_become_a_whole_row_argument {
        use crate::test_helpers::{
            query::expression, query_whole_rows as test_helpers, query_whole_rows::catalog_query,
        };

        #[test]
        fn aggregate_star_does_not_become_a_whole_row_argument() {
            let (query, context) = catalog_query("SELECT count(*) FROM pg_catalog.pg_type t");
            assert_eq!(context.reported_problem_count(), 0);
            assert_eq!(
                expression(&query).as_function_call().unwrap().args().len(),
                0
            );
        }

        #[test]
        fn a_bare_call_star_does_not_select_an_implicit_row() {
            let (query, context) = catalog_query("SELECT identity(*) FROM pg_catalog.pg_type t");
            test_helpers::assert_problem(&context, "function_overload_resolution_failed");
            assert_eq!(
                expression(&query).as_function_call().unwrap().args().len(),
                0
            );
        }
    }
}

pub use self::implementation::{FuncCallArgs, FuncCallExpr, FunctionBinding};
