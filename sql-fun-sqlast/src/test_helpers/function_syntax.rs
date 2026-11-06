use crate::{
    ast::{
        Expr, Name, TryFromWithTokens, TypeName,
        ddl::{FunctionParameter, ParameterMode},
    },
    sem::AnalysisError,
    syn::{KeywordKindOpt, ScanToken, ScanTokenBuilder, Token, TokenOpt},
    test_helpers::SynBuilder,
};
use ::sql_fun_core::IVec;

pub(crate) fn func_name_tokens() -> IVec<ScanToken> {
    let token = ScanTokenBuilder::default()
        .start(1)
        .end(1)
        .token(TokenOpt::from(Token::Ident))
        .keyword_kind(KeywordKindOpt::none())
        .build()
        .unwrap();
    vec![token.into()].into()
}

/// Build a common-AST function parameter without parser-specific nodes.
pub(crate) fn parameter(name: &str, mode: ParameterMode) -> FunctionParameter {
    FunctionParameter::new(
        name,
        TypeName::projection(&Name::from_parts(&["int4".to_owned()]), false),
        mode,
        None,
    )
}

/// Preserve the parser-built function-call fixture and its source-token setup.
pub(crate) fn call(name: &str) -> Result<Expr, AnalysisError> {
    use crate::syn::{Node, NodeInner, NodeList};
    let builder = SynBuilder::new();
    let arg = Node::from(NodeInner::AConst(builder.const_int4(1)));
    let args = NodeList::from(vec![arg]);
    let name = NodeList::from(vec![builder.as_string_node(builder.string(name))]);
    let call = builder.func_call(name, args);
    Expr::try_from_with_tokens(&Node::from(NodeInner::from(call)), &func_name_tokens())
}
