use crate::sem::{
    AliasName, AnalysisError, AnalysisProblem, ColumnName, DataSource, FullName, ParseContext,
    WithClause,
    data_source::{AliasSpec, QualifiedSource},
};
use ::sql_fun_core::LookupResult;

/// Query-local namespace entries retain occurrences, rather than just local names.
#[derive(Debug, Default)]
pub(crate) struct SourceNamespace<'a> {
    bindings: Vec<SourceBinding<'a>>,
}

#[derive(Debug, Clone)]
struct SourceBinding<'a> {
    alias: AliasSpec,
    source: QualifiedSource<'a>,
    relation: Option<FullName>,
    unqualified: bool,
}

impl<'a> SourceBinding<'a> {
    fn new(source: &'a DataSource, alias: &AliasSpec) -> Self {
        Self {
            alias: alias.clone(),
            source: QualifiedSource::new(source, alias.name()),
            relation: source.unaliased_relation(alias),
            unqualified: true,
        }
    }

    fn conflicts(&self, other: &Self, context: &impl ParseContext) -> bool {
        if self.alias.name() != other.alias.name() {
            return false;
        }
        if context.dialect() == crate::ast::Dialect::postgres()
            && let (Some(left), Some(right)) = (&self.relation, &other.relation)
        {
            return left == right;
        }
        true
    }
}

impl<'a> SourceNamespace<'a> {
    pub(super) fn push(&mut self, source: &'a DataSource, alias: &AliasSpec) {
        self.bindings.push(SourceBinding::new(source, alias));
    }

    pub(crate) fn append(&mut self, mut other: Self) {
        self.bindings.append(&mut other.bindings);
    }

    pub(super) fn with_nullability(mut self, nullable: bool) -> Self {
        self.bindings = self
            .bindings
            .into_iter()
            .map(|mut binding| {
                binding.source = binding.source.with_nullability(nullable);
                binding
            })
            .collect();
        self
    }

    pub(super) fn with_merge(mut self, role: super::MergeRecordRole, nullable: bool) -> Self {
        self.bindings = self
            .bindings
            .into_iter()
            .map(|mut binding| {
                binding.source = binding.source.with_merge(role, nullable);
                binding
            })
            .collect();
        self
    }

    pub(super) fn hide_group_columns(&mut self) {
        for binding in &mut self.bindings {
            binding.unqualified = false;
        }
    }

    pub(crate) fn validate_against(
        &self,
        previous: &Self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        for binding in &self.bindings {
            if previous
                .bindings
                .iter()
                .any(|other| binding.conflicts(other, context))
            {
                context.report_analysis_problem(AnalysisProblem::duplicate_alias(
                    binding.alias.name(),
                ))?;
            }
        }
        Ok(())
    }

    pub(crate) fn contains_alias(&self, name: &AliasName) -> bool {
        self.bindings
            .iter()
            .any(|binding| binding.alias.name() == name)
    }

    pub(crate) fn resolve_name(&self, name: &AliasName) -> Result<Option<QualifiedSource<'a>>, ()> {
        let mut result = None;
        for binding in &self.bindings {
            if binding.alias.name() == name {
                if result.is_some() {
                    return Err(());
                }
                result = Some(binding.source.clone());
            }
        }
        Ok(result)
    }

    pub(crate) fn resolve_column(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        column: &ColumnName,
    ) -> Result<Result<LookupResult<QualifiedSource<'a>>, ()>, AnalysisError> {
        let mut result = None;
        let mut unknown = false;
        for binding in &self.bindings {
            if !binding.unqualified {
                continue;
            }
            let candidates = binding.source.column_candidates(context, with, column)?;
            match candidates.resolve() {
                Err(_) => return Ok(Err(())),
                Ok(LookupResult::Found(_)) => {
                    if result.is_some() {
                        return Ok(Err(()));
                    }
                    result = Some(binding.source.clone());
                }
                Ok(LookupResult::Unknown) => unknown = true,
                Ok(LookupResult::NotFound) => {}
            }
        }
        if unknown {
            Ok(Ok(LookupResult::unknown()))
        } else if let Some(source) = result {
            Ok(Ok(LookupResult::found(source)))
        } else {
            Ok(Ok(LookupResult::not_found()))
        }
    }
}

impl DataSource {
    pub(super) fn unaliased_relation(&self, alias: &AliasSpec) -> Option<FullName> {
        if alias.is_explicit() {
            return None;
        }
        match self {
            Self::Table(table) => Some(table.name().full_name().clone()),
            Self::View(view) => Some(view.view_name().full_name().clone()),
            Self::RangeTableSample(sample) => sample.source().unaliased_relation(alias),
            _ => None,
        }
    }

    pub(crate) fn namespace(&self) -> SourceNamespace<'_> {
        match self {
            Self::RangeTableSample(sample) => sample.source().namespace(),
            Self::JoinExpr(join) => join.namespace(self),
            Self::Group(group) => group.namespace(self),
            Self::MergeRecord(record) => record
                .source()
                .namespace()
                .with_merge(record.role(), record.is_nullable()),
            _ => {
                let mut namespace = SourceNamespace::default();
                for alias in self.get_aliases() {
                    namespace.push(self, &alias);
                }
                namespace
            }
        }
    }
}
