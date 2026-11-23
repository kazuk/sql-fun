//! Unresolved PostgreSQL cast declarations.

use sql_fun_core::IVec;

use crate::{
    ast::{CastTarget, ObjectTarget, TryFromWithTokens, TypeName, postgres::required},
    error::AnalysisError,
    syn::{self, CoercionContext, Opt, ScanToken},
};

// Retained for semantic interpretation (#1237).
#[derive(Clone, Debug)]
#[allow(dead_code, reason = "CREATE CAST interpretation is deferred to #1237")]
pub(crate) struct CreateCast {
    types: CastTarget,
    method: CastMethod,
    context: CastContext,
}

#[derive(Clone, Debug)]
#[allow(dead_code, reason = "CREATE CAST interpretation is deferred to #1237")]
enum CastMethod {
    Function(Box<ObjectTarget>),
    Binary,
    InOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CastContext {
    Explicit,
    Assignment,
    Implicit,
}

impl TryFromWithTokens<&syn::CreateCastStmt> for CreateCast {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::CreateCastStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let source = required(value.get_sourcetype().as_inner(), "CREATE CAST source type")?;
        let target = required(value.get_targettype().as_inner(), "CREATE CAST target type")?;
        let types = CastTarget {
            source: TypeName::try_from_with_tokens(&source, tokens)?,
            target: TypeName::try_from_with_tokens(&target, tokens)?,
        };
        let method = match (value.get_func().as_inner(), value.get_inout()) {
            (Some(function), false) => CastMethod::Function(Box::new(
                ObjectTarget::try_from_object_with_args(&function, tokens)?,
            )),
            (None, false) => CastMethod::Binary,
            (None, true) => CastMethod::InOut,
            (Some(_), true) => {
                return AnalysisError::raise_unexpected_input("CREATE CAST implementation");
            }
        };
        let context = match value.get_context().as_inner() {
            Some(CoercionContext::CoercionExplicit) => CastContext::Explicit,
            Some(CoercionContext::CoercionAssignment) => CastContext::Assignment,
            Some(CoercionContext::CoercionImplicit) => CastContext::Implicit,
            _ => return AnalysisError::raise_unexpected_input("CREATE CAST context"),
        };
        Ok(Self {
            types,
            method,
            context,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ast::{
            Dialect, StatementKind, StatementNode,
            create_cast::{CastContext, CastMethod},
        },
        parse_statements,
    };

    #[rstest::rstest]
    #[case(
        "CREATE CAST (citext AS text)    WITHOUT FUNCTION AS IMPLICIT",
        "citext",
        "text",
        CastContext::Implicit,
        false
    )]
    #[case(
        "CREATE CAST (app.source_type AS text) WITH INOUT AS ASSIGNMENT",
        "source_type",
        "text",
        CastContext::Assignment,
        true
    )]
    fn retains_cast_declaration(
        #[case] declaration: &str,
        #[case] source: &str,
        #[case] target: &str,
        #[case] context: CastContext,
        #[case] inout: bool,
    ) {
        let sql = format!("SELECT 1;\n{declaration};");
        let statement = parse_statements(&sql, Dialect::postgres())
            .nth(1)
            .unwrap()
            .unwrap();
        assert_eq!(statement.kind(), StatementKind::CreateCast);
        assert_eq!(
            statement
                .span()
                .get_from(&sql)
                .unwrap()
                .trim()
                .trim_end_matches(';'),
            declaration
        );
        let StatementNode::CreateCast(cast) = statement.node() else {
            panic!("expected CREATE CAST");
        };
        assert_eq!(cast.types.source.name().parts().last().unwrap(), source);
        assert_eq!(cast.types.target.name().parts(), &[target]);
        assert_eq!(cast.context, context);
        if inout {
            assert!(matches!(cast.method, CastMethod::InOut));
            assert_eq!(cast.types.source.name().parts(), &["app", source]);
        } else {
            assert!(matches!(cast.method, CastMethod::Binary));
        }
    }

    #[rstest::rstest]
    #[case(
        "CREATE CAST (text[] AS hstore)\n  WITH FUNCTION hstore(text[]);",
        &["hstore"]
    )]
    #[case(
        "CREATE CAST (text[] AS hstore) WITH FUNCTION app.hstore(text[]);",
        &["app", "hstore"]
    )]
    fn retains_cast_function_signature(#[case] sql: &str, #[case] name: &[&str]) {
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(statement.kind(), StatementKind::CreateCast);
        assert_eq!(
            statement
                .span()
                .get_from(sql)
                .unwrap()
                .trim_end_matches(';'),
            sql.trim_end_matches(';')
        );
        let StatementNode::CreateCast(cast) = statement.node() else {
            panic!("expected CREATE CAST");
        };
        let CastMethod::Function(function) = &cast.method else {
            panic!("expected a cast function");
        };
        assert_eq!(cast.types.source.name().parts(), &["text"]);
        assert_eq!(cast.types.source.array_bounds(), &[None]);
        assert_eq!(cast.types.target.name().parts(), &["hstore"]);
        assert_eq!(cast.context, CastContext::Explicit);
        assert_eq!(function.name(), name);
        let [argument] = function.argument_types().as_deref().unwrap() else {
            panic!("expected one function argument");
        };
        let argument = argument.type_name().as_ref().unwrap();
        assert_eq!(argument.name().parts(), &["text"]);
        assert_eq!(argument.array_bounds(), &[None]);
    }
}
