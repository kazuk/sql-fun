//! Source-independent explanations for structured SQL diagnostics.
use std::fmt::Write;

use crate::analysis_problem::{
    AlterColumnProblem, ArrayProblem, CallArgumentProblem, ColumnAttributeProblem, CreationProblem,
    DeleteProblem, DiagnosticMessage, DiagnosticMessageFragment, IndexProblem, InsertProblem,
    MinMaxProblem, OwnershipProblem, ParameterConstraintProblem, QueryProblem,
    RuleDefinitionProblem, RuleProblem, TableFunctionProblem, TriggerProblem, UpdateProblem,
    ViewProblem, fragments_text,
};

impl DiagnosticMessage for ArrayProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::Empty => {
                "cannot determine type of empty ARRAY; supply an explicit array cast".to_owned()
            }
            Self::Incompatible(left, right) => {
                format!("ARRAY cannot match or convert {left} and {right}")
            }
            Self::MissingTypeFacts(ty) => format!("ARRAY requires type-category metadata for {ty}"),
            Self::SubqueryColumns(count) => {
                format!("ARRAY subquery must return exactly one column, found {count}")
            }
        }
    }
}

impl DiagnosticMessage for MinMaxProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::Empty => "GREATEST/LEAST requires at least one argument".to_owned(),
            Self::Incompatible(left, right) => {
                format!("GREATEST/LEAST cannot match {left} and {right}")
            }
            Self::MissingTypeFacts(ty) => {
                format!("GREATEST/LEAST requires type-category metadata for {ty}")
            }
            Self::MissingComparisonFacts(ty) => {
                format!("GREATEST/LEAST requires comparison metadata for {ty}")
            }
            Self::NotOrdered(ty) => {
                format!("GREATEST/LEAST cannot identify a comparison function for {ty}")
            }
        }
    }
}

impl DiagnosticMessage for CallArgumentProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::UnknownName(name) => {
                format!("no function candidate recognizes argument name \"{name}\"")
            }
            Self::MissingRequired(Some(name)) => {
                format!("required argument \"{name}\" was not supplied")
            }
            Self::MissingRequired(None) => {
                "a required unnamed argument was not supplied".to_owned()
            }
            Self::AssignedTwice(name) => {
                format!("argument \"{name}\" was assigned both positionally and by name")
            }
            Self::DuplicateName(name, _) => format!("argument name \"{name}\" used more than once"),
            Self::PositionalAfterNamed(position, _) => format!(
                "positional argument at call position {position} cannot follow named argument"
            ),
        }
    }
}

impl DiagnosticMessage for IndexProblem {
    fn diagnostic_message(&self) -> String {
        fragments_text(&self.diagnostic_fragments())
    }

    fn diagnostic_fragments(&self) -> Vec<DiagnosticMessageFragment<'_>> {
        let mut suffix = format!(": {}", self.kind().message());
        if let Some(parameter) = self.parameter_name() {
            let _ = write!(suffix, " ({parameter})");
        }
        if let Some(column) = self.column() {
            let _ = write!(suffix, " (column {column})");
        }
        if let Some(value) = self.value() {
            let _ = write!(suffix, ": {value}");
        }
        vec![self.name().into(), suffix.into()]
    }
}

impl DiagnosticMessage for AlterColumnProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::DropLastColumn => "SQLite requires at least one non-generated column".to_owned(),
            Self::DropDependency(identity) => format!("column removal is prevented by {}.{}", identity.relation(), identity.member()),
            Self::AddNullability => "added column has conflicting NULL/NOT NULL declarations".to_owned(),
            Self::AddSqliteKey => "SQLite ADD COLUMN cannot declare PRIMARY KEY or UNIQUE".to_owned(),
            Self::AddPrimaryKey => "a table cannot have more than one primary key".to_owned(),
            Self::AddDefault => "added column default requires a resolved scalar expression assignable to the declared type".to_owned(),
            Self::DefinitionSyntax => "SQLite does not support ALTER COLUMN ... TO column-definition".to_owned(),
            Self::DuplicateType => "cannot alter the type of one column twice".to_owned(),
            Self::TypeConversion => "column value cannot be assigned to the requested type".to_owned(),
            Self::DefaultConversion => "default cannot be assigned to the requested type independently of USING".to_owned(),
            Self::UsingExpression => "USING requires a resolved scalar expression without subqueries, parameters, aggregates or window/set-returning functions".to_owned(),
            Self::InvalidTypmod(message) => format!("invalid requested type modifier: {message}"),
            Self::MissingRelation => "ALTER COLUMN target is absent or is not a table".to_owned(),
            Self::MissingColumn => "ALTER COLUMN target column does not exist".to_owned(),
            Self::PrimaryKey => "a primary-key column must remain NOT NULL".to_owned(),
            Self::Identity => "an identity column must remain NOT NULL".to_owned(),
            Self::NotStoredGenerated => {
                "DROP EXPRESSION requires a stored generated column".to_owned()
            }
            Self::VirtualGenerated => {
                "this operation is not supported for a virtual generated column".to_owned()
            }
            Self::Version => {
                "this ALTER COLUMN spelling is not supported by the selected PostgreSQL version"
                    .to_owned()
            }
            Self::StatisticsTooLow(value) => format!("statistics target {value} is below -1"),
            Self::StatisticsReduced(value) => {
                format!("statistics target {value} is reduced to 10000")
            }
            Self::UnknownOption(name) => format!("unknown column attribute option {name}"),
            Self::DuplicateOption(name) => {
                format!("column attribute option {name} is specified more than once")
            }
            Self::InvalidOption(name) => {
                format!("invalid value for column attribute option {name}")
            }
            Self::ResetValue(name) => format!("RESET option {name} must not include a value"),
        }
    }
}

impl DiagnosticMessage for ColumnAttributeProblem {
    fn diagnostic_message(&self) -> String {
        fragments_text(&self.diagnostic_fragments())
    }

    fn diagnostic_fragments(&self) -> Vec<DiagnosticMessageFragment<'_>> {
        vec![
            self.table().into(),
            ".".into(),
            self.column().into(),
            format!(": {}", self.reason().diagnostic_message()).into(),
        ]
    }
}

impl DiagnosticMessage for CreationProblem {
    fn diagnostic_message(&self) -> String {
        fragments_text(&self.diagnostic_fragments())
    }

    fn diagnostic_fragments(&self) -> Vec<DiagnosticMessageFragment<'_>> {
        if let (Some(requested), Some(existing)) = (self.requested_kind(), self.existing_kind()) {
            vec![
                format!("cannot create {requested} ").into(),
                self.name().into(),
                format!(": name is occupied by {existing}").into(),
            ]
        } else if let Some(container) = self.container() {
            vec![
                "constraint ".into(),
                self.name().into(),
                " already exists on ".into(),
                container.into(),
            ]
        } else {
            vec![
                format!("conflicting {} definition: ", self.namespace()).into(),
                self.name().into(),
            ]
        }
    }
}

impl DiagnosticMessage for RuleDefinitionProblem {
    fn diagnostic_message(&self) -> String {
        format!("rule {} is invalid: {}", self.identity(), self.detail())
    }
}

impl DiagnosticMessage for RuleProblem {
    fn diagnostic_message(&self) -> String {
        fragments_text(&self.diagnostic_fragments())
    }

    fn diagnostic_fragments(&self) -> Vec<DiagnosticMessageFragment<'_>> {
        match self {
            Self::Target(target) => vec![
                "rule target ".into(),
                target.into(),
                " does not resolve to a supported relation".into(),
            ],
            Self::Duplicate(identity) => vec![format!("rule {identity} already exists").into()],
            Self::Missing(identity) => vec![format!("rule {identity} does not exist").into()],
            Self::Definition(problem) => problem.diagnostic_fragments(),
        }
    }
}

impl DiagnosticMessage for OwnershipProblem {
    fn diagnostic_message(&self) -> String {
        if let Some(role) = self.role_name() {
            format!("owner role does not exist: {role}")
        } else if let Some(target) = self.target_outcome() {
            target.diagnostic_message()
        } else {
            "ownership target or role could not be resolved".to_owned()
        }
    }
}

impl DiagnosticMessage for TriggerProblem {
    fn diagnostic_message(&self) -> String {
        fragments_text(&self.diagnostic_fragments())
    }

    fn diagnostic_fragments(&self) -> Vec<DiagnosticMessageFragment<'_>> {
        match self {
            Self::Target(target) => vec![
                "trigger target ".into(),
                target.into(),
                " does not resolve to a supported relation".into(),
            ],
            Self::Duplicate(identity) => vec![format!("trigger {identity} already exists").into()],
            Self::Missing(identity) => vec![format!("trigger {identity} does not exist").into()],
            Self::MissingName(name) => {
                vec!["trigger ".into(), name.into(), " does not exist".into()]
            }
            Self::Column(target, column) => vec![
                format!("event column {column} does not exist on trigger target ").into(),
                target.into(),
            ],
            Self::Function(name) => vec![
                "trigger function ".into(),
                name.into(),
                " is missing or is not usable as a trigger function".into(),
            ],
            Self::Options(target, rule) => vec![
                "trigger ".into(),
                target.into(),
                format!(" has incompatible options: {rule}").into(),
            ],
        }
    }
}

impl DiagnosticMessage for ViewProblem {
    fn diagnostic_message(&self) -> String {
        fragments_text(&self.diagnostic_fragments())
    }

    fn diagnostic_fragments(&self) -> Vec<DiagnosticMessageFragment<'_>> {
        match self {
            Self::InvalidTarget(view) => vec![
                "declaration target ".into(),
                view.into(),
                " is not an ordinary view".into(),
            ],
            Self::UnknownDefaultColumn(view, column) => vec![
                format!("default column {column} does not exist on ").into(),
                view.into(),
            ],
            Self::MissingMetadata(view) => vec![
                "the semantic definition required for ".into(),
                view.into(),
                " is missing; reload the complete schema declarations".into(),
            ],
            Self::InvalidAlter => {
                vec!["PostgreSQL does not permit ALTER trigger/rule state on views".into()]
            }
        }
    }
}

impl DiagnosticMessage for InsertProblem {
    fn diagnostic_message(&self) -> String {
        self.message().to_owned()
    }
}

impl DiagnosticMessage for DeleteProblem {
    fn diagnostic_message(&self) -> String {
        self.message()
    }

    fn diagnostic_fragments(&self) -> Vec<DiagnosticMessageFragment<'_>> {
        self.fragments()
    }
}

impl DiagnosticMessage for UpdateProblem {
    fn diagnostic_message(&self) -> String {
        self.message().to_owned()
    }
}

impl DiagnosticMessage for QueryProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::SetWidth(_) | Self::SetType(_) => self.render_metadata().message().to_owned(),
            Self::ScalarValueRequired => "expression requires a scalar value".to_owned(),
            Self::ColumnDatabaseNameRequired(reference) => format!("database-name metadata is required for column reference: {reference}"),
            Self::ColumnCrossDatabase(reference) => format!("cross-database column reference is not supported: {reference}"),
            Self::ColumnQualificationInvalid(reference) => format!("column qualification is invalid for the selected dialect: {reference}"),
            Self::ColumnSourceNotFound(reference) => format!("visible source not found for column reference: {reference}"),
            Self::ColumnSourceAmbiguous(reference) => format!("ambiguous visible source for column reference: {reference}"),
            Self::GroupingArgumentNotGrouped => "arguments to GROUPING must be grouping expressions of the associated query level".to_owned(),
            Self::InvalidGroupingPlacement(placement) => format!("grouping operations are not allowed in {placement}"),
            Self::TooManyGroupingArguments => "GROUPING must have fewer than 32 arguments".to_owned(),
            Self::InvalidCtePlacement => "data-modifying WITH must be at the top level; nested WITH bodies cannot contain modifying WITH or SELECT INTO".to_owned(),
            Self::DuplicateWindowDefinition(name) => format!("duplicate window definition: {name}"),
            Self::ReferenceNotFound(name) => format!("clause reference not found: {name}"),
            Self::AmbiguousReference(name) => format!("ambiguous clause reference: {name}"),
            Self::PositionOutOfRange(position) => format!("ORDER/GROUP BY position {position} is outside the select list"),
            Self::InvalidCompoundOrder => "compound ORDER BY must refer to a result column".to_owned(),
            Self::StarProjectionRequiresCompositeType => {
                "row expansion requires a composite value".to_owned()
            }
            Self::InvalidStarProjectionPlacement => {
                "row expansion is only valid in a projection target".to_owned()
            }
        }
    }
}

impl DiagnosticMessage for TableFunctionProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::InvalidExpression => "aggregate, window or nested set-returning expression is not allowed in a FROM function".to_owned(),
            Self::InvalidTypmod(message) => format!("invalid table-function column type modifier: {message}"),
            Self::InvalidPlacement => "outer column definitions require one function, no per-function definitions, and no WITH ORDINALITY".to_owned(),
            Self::DefinitionRequired => "a column definition list is required for functions returning record".to_owned(),
            Self::DefinitionNotAllowed => "a column definition list is only allowed for functions returning record without declared output columns".to_owned(),
            Self::DuplicateColumn(name) => format!("table-function column {name} specified more than once"),
            Self::TypeNotFound(name) => format!("table-function column type not found: {name}"),
            Self::CollationNotFound(name) => format!("table-function collation not uniquely resolved: {name}"),
            Self::TooManyAliases => "table-function column alias list has more entries than output columns".to_owned(),
        }
    }
}

impl DiagnosticMessage for ParameterConstraintProblem {
    fn diagnostic_message(&self) -> String {
        self.message().to_owned()
    }
}
