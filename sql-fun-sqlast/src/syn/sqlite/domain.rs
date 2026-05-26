use crate::syn::sqlite::Expr;

#[derive(Clone, derive_getters::Getters)]
pub struct DomainConstraint {
    name: Option<String>,
    check: Box<Expr>,
}

use turso_parser::ast::DomainConstraint as DomainConstraintIn;

impl From<DomainConstraintIn> for DomainConstraint {
    fn from(value: DomainConstraintIn) -> Self {
        Self {
            name: value.name,
            check: Box::new(Expr::from(*value.check)),
        }
    }
}

/// create domain statement
#[derive(derive_getters::Getters)]
pub struct CreateDomainStatement {
    /// `IF NOT EXISTS`
    if_not_exists: bool,

    /// domain name
    domain_name: String,

    /// base type
    base_type: String,

    /// default expression
    default: Option<Box<Expr>>,

    /// NOT NULL constraint
    not_null: bool,

    /// check constraints
    constraints: Vec<DomainConstraint>,
}

impl CreateDomainStatement {
    /// create instance
    pub fn new(
        if_not_exists: bool,
        domain_name: &str,
        base_type: &str,
        default: Option<Expr>,
        not_null: bool,
        constraints: Vec<DomainConstraint>,
    ) -> Self {
        Self {
            if_not_exists,
            domain_name: String::from(domain_name),
            base_type: String::from(base_type),
            default: default.map(Box::new),
            not_null,
            constraints,
        }
    }
}

/// drop domain statement
#[derive(derive_getters::Getters)]
pub struct DropDomainStatement {
    if_exists: bool,

    domain_name: String,
}

impl DropDomainStatement {
    /// create instance
    pub fn new(if_exists: bool, domain_name: &str) -> Self {
        Self {
            if_exists,
            domain_name: String::from(domain_name),
        }
    }
}
