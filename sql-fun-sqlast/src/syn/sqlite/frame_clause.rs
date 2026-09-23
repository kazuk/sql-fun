use crate::syn::sqlite::Expr;

use turso_parser::ast::FrameClause as FrameClauseIn;

/// frame clause
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct FrameClause {
    mode: FrameMode,
    start: FrameBound,
    end: Option<FrameBound>,
    exclude: Option<FrameExclude>,
}

impl From<FrameClauseIn> for FrameClause {
    fn from(value: FrameClauseIn) -> Self {
        Self {
            mode: FrameMode::from(value.mode),
            start: FrameBound::from(value.start),
            end: value.end.map(FrameBound::from),
            exclude: value.exclude.map(FrameExclude::from),
        }
    }
}

use turso_parser::ast::FrameMode as FrameModeIn;

/// frame mode
#[derive(Clone, Debug)]
pub enum FrameMode {
    /// groups
    Groups,
    /// range
    Range,
    /// rows
    Rows,
}

impl From<FrameModeIn> for FrameMode {
    fn from(value: FrameModeIn) -> Self {
        match value {
            FrameModeIn::Groups => Self::Groups,
            FrameModeIn::Range => Self::Range,
            FrameModeIn::Rows => Self::Rows,
        }
    }
}

use turso_parser::ast::FrameBound as FrameBoundIn;

/// frame bound
#[derive(Clone, Debug)]
pub enum FrameBound {
    /// current row
    CurrentRow,
    /// following
    Following(Box<Expr>),
    /// preceding
    Preceding(Box<Expr>),
    /// unbounded following
    UnboundedFollowing,
    /// unbounded preceding
    UnboundedPreceding,
}

impl From<FrameBoundIn> for FrameBound {
    fn from(value: FrameBoundIn) -> Self {
        match value {
            FrameBoundIn::CurrentRow => Self::CurrentRow,
            FrameBoundIn::Following(expr) => Self::Following(Box::new(Expr::from(*expr))),
            FrameBoundIn::Preceding(expr) => Self::Preceding(Box::new(Expr::from(*expr))),
            FrameBoundIn::UnboundedFollowing => Self::UnboundedFollowing,
            FrameBoundIn::UnboundedPreceding => Self::UnboundedPreceding,
        }
    }
}

use turso_parser::ast::FrameExclude as FrameExcludeIn;

/// frame exclude
#[derive(Clone, Debug)]
pub enum FrameExclude {
    /// no-others
    NoOthers,
    /// current row
    CurrentRow,
    /// group
    Group,
    /// ties
    Ties,
}

impl From<FrameExcludeIn> for FrameExclude {
    fn from(value: FrameExcludeIn) -> Self {
        match value {
            FrameExcludeIn::NoOthers => Self::NoOthers,
            FrameExcludeIn::CurrentRow => Self::CurrentRow,
            FrameExcludeIn::Group => Self::Group,
            FrameExcludeIn::Ties => Self::Ties,
        }
    }
}
