use crate::{
    StringSpan,
    ast::{
        Literal, Name, TryFromWithTokens, TypeName,
        ddl::FunctionParameter,
        postgres::{anchored_token_span, required},
    },
    error::AnalysisError,
    syn::{self, ListOpt, Opt, ScanToken},
};
use sql_fun_core::IVec;

#[derive(Clone, Debug)]
pub(crate) struct CreateAggregate {
    pub(crate) name: Name,
    pub(crate) replace: bool,
    pub(crate) arguments: AggregateArguments,
    pub(crate) options: Vec<AggregateOption>,
}

#[derive(Clone, Debug)]
pub(crate) enum AggregateArguments {
    Legacy,
    Declared {
        parameters: Vec<FunctionParameter>,
        // Equality with the parameter count denotes a shared variadic parameter.
        direct_count: Option<usize>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct AggregateOption {
    pub(crate) key: String,
    pub(crate) value: Option<AggregateOptionValue>,
    pub(crate) location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) enum AggregateOptionValue {
    Type(TypeName),
    Name(Name),
    Literal(Literal),
}

impl TryFromWithTokens<&syn::DefineStmt> for CreateAggregate {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        node: &syn::DefineStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            name: Name::try_from(node.get_defnames())?,
            replace: node.get_replace(),
            arguments: AggregateArguments::from_definition(node, tokens)?,
            options: node
                .get_definition()
                .as_inner()
                .unwrap_or_default()
                .iter()
                .map(|option| AggregateOption::try_from_with_tokens(option, tokens))
                .collect::<Result<_, _>>()?,
        })
    }
}

impl AggregateArguments {
    fn from_definition(
        node: &syn::DefineStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, AnalysisError> {
        if node.get_oldstyle() {
            return Ok(Self::Legacy);
        }
        let args = required(node.get_args().as_inner(), "aggregate arguments")?;
        let [parameters, count] = args.as_slice() else {
            return AnalysisError::raise_unexpected_input("aggregate argument layout");
        };
        let count = required(
            count.as_integer().get_ival(),
            "aggregate direct argument count",
        )?;
        let parameters = parameters
            .as_list()
            .get_items()
            .as_inner()
            .unwrap_or_default()
            .iter()
            .map(|argument| {
                let parameter = required(
                    argument.as_function_parameter().as_inner(),
                    "aggregate argument",
                )?;
                FunctionParameter::try_from_with_tokens(&parameter, tokens)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::Declared {
            parameters,
            direct_count: usize::try_from(count).ok(),
        })
    }
}

impl TryFromWithTokens<&syn::Node> for AggregateOption {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        node: &syn::Node,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let option = required(node.as_def_elem().as_inner(), "aggregate option")?;
        let key = option.get_defname();
        let value = option
            .get_arg()
            .as_inner()
            .map(|arg| {
                if let Some(name) = arg.as_type_name().as_inner() {
                    if matches!(key.as_str(), "stype" | "mstype" | "basetype") {
                        return TypeName::try_from_with_tokens(&name, tokens)
                            .map(AggregateOptionValue::Type);
                    }
                    return Name::try_from(name.get_names()).map(AggregateOptionValue::Name);
                }
                if let Some(name) = arg.as_list().as_inner() {
                    return Name::try_from(name.get_items()).map(AggregateOptionValue::Name);
                }
                required(Option::<Literal>::from(&arg), "aggregate option value")
                    .map(AggregateOptionValue::Literal)
            })
            .transpose()?;
        Ok(Self {
            key,
            value,
            location: anchored_token_span(option.get_location(), tokens),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ast::{Dialect, StatementNode},
        parse_statements,
    };

    #[test]
    fn converts_bundled_aggregate_declarations() {
        for script in [
            include_str!("../../../sql-fun-home/postgres/17/extension/citext--1.4.sql"),
            include_str!("../../../sql-fun-home/postgres/18/extension/citext--1.4.sql"),
            include_str!("../../../sql-fun-home/postgres/17/extension/intagg--1.1.sql"),
            include_str!("../../../sql-fun-home/postgres/18/extension/intagg--1.1.sql"),
        ] {
            let declarations = script
                .split("CREATE AGGREGATE ")
                .skip(1)
                .collect::<Vec<_>>();
            assert!(!declarations.is_empty());
            for declaration in declarations {
                let sql = format!(
                    "CREATE AGGREGATE {};",
                    declaration.split_once(';').unwrap().0
                );
                let statement = parse_statements(&sql, Dialect::postgres())
                    .next()
                    .unwrap()
                    .unwrap();
                let StatementNode::CreateAggregate(aggregate) = statement.node() else {
                    panic!("expected CREATE AGGREGATE");
                };
                assert!(
                    aggregate
                        .options
                        .iter()
                        .all(|option| option.location.is_some())
                );
            }
        }
    }
}
