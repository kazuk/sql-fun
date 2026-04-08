use turso_parser::ast::Window as WindowIn;

use crate::syn::sqlite::{Expr, FrameClause, Name, SortedColumn};

/// window
#[derive(Debug, derive_getters::Getters)]
pub struct Window {
    base: Option<Name>,
    partition_by: Vec<Box<Expr>>,
    order_by: Vec<SortedColumn>,
    frame_clause: Option<FrameClause>,
}

impl Window {
    /// create new Window
    pub fn new(
        base: Option<Name>,
        partition_by: Vec<Expr>,
        order_by: Vec<SortedColumn>,
        frame_clause: Option<FrameClause>,
    ) -> Self {
        Self {
            base,
            partition_by: partition_by.into_iter().map(Box::new).collect(),
            order_by,
            frame_clause,
        }
    }
}

impl From<WindowIn> for Window {
    fn from(value: WindowIn) -> Self {
        Self::new(
            value.base.map(Name::from),
            value
                .partition_by
                .into_iter()
                .map(|v| Expr::from(*v))
                .collect(),
            value.order_by.into_iter().map(SortedColumn::from).collect(),
            value.frame_clause.map(FrameClause::from),
        )
    }
}

use turso_parser::ast::WindowDef as WindowDefIn;

/// window def
#[derive(Debug, derive_getters::Getters)]
pub struct WindowDef {
    name: Name,
    window: Window,
}

impl From<WindowDefIn> for WindowDef {
    fn from(value: WindowDefIn) -> Self {
        Self {
            name: Name::from(value.name),
            window: Window::from(value.window),
        }
    }
}
