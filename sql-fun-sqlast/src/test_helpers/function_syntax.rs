use crate::syn::{KeywordKindOpt, ScanToken, ScanTokenBuilder, Token, TokenOpt};
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
