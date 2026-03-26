use turso_parser::ast::Over as OverIn;

use crate::syn::sqlite::{Name, Window};

/// over clause
pub enum Over {
    /// window
    Window(Window),
    /// name
    Name(Name),
}

impl From<OverIn> for Over {
    fn from(value: OverIn) -> Self {
        match value {
            OverIn::Window(window) => Self::Window(Window::from(window)),
            OverIn::Name(name) => Self::Name(Name::from(name)),
        }
    }
}
