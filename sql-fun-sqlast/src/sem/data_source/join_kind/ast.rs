use crate::{ast::JoinKind, sem::JoinKind as SemanticJoinKind};

impl SemanticJoinKind {
    pub(crate) const fn from_ast(ast: &JoinKind) -> Self {
        match ast {
            JoinKind::Inner | JoinKind::Cross => Self::inner(),
            JoinKind::Left => Self::left(),
            JoinKind::Right => Self::right(),
            JoinKind::Full => Self::full(),
        }
    }
}
