use crate::{ast::DropBehavior, sem::DropDependencyBehavior};

impl DropDependencyBehavior {
    pub(crate) const fn from_ast(ast: &DropBehavior) -> Self {
        match ast {
            DropBehavior::Cascade => Self::cascade(),
            DropBehavior::Restrict => Self::restrict(),
        }
    }
}
