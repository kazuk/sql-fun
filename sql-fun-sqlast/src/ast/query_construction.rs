//! Common construction of explicit query projections and their source records.
use crate::ast::{
    Access, Alias, Call, Cast, DerivedTable, Expr, FromItem, Indirection, Name, Query, QueryBody,
    Select, Target, TypeName, With,
};
impl Select {
    pub(in crate::ast) fn projection(targets: &[Target], from: &[FromItem]) -> Self {
        Self {
            into: None,
            distinct: None,
            targets: targets.to_vec(),
            from: from.to_vec(),
            predicate: None,
            group: Vec::new(),
            group_distinct: false,
            having: None,
            windows: Vec::new(),
        }
    }
}
impl Query {
    pub(in crate::ast) fn projection(targets: &[Target], from: &[FromItem]) -> Self {
        Self {
            with: With::default(),
            body: QueryBody::projection_select(&Select::projection(targets, from)),
            order: Vec::new(),
            limit: None,
            offset: None,
            with_ties: false,
            locks: Vec::new(),
        }
    }
}
impl DerivedTable {
    pub(in crate::ast) fn lateral_projection(query: &Query, alias: &str) -> Self {
        Self {
            query: query.clone(),
            alias: Some(Alias::projection(alias)),
            lateral: true,
        }
    }
}
impl Alias {
    pub(in crate::ast) fn projection(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            columns: Vec::new(),
        }
    }
}
impl Target {
    pub(in crate::ast) const fn source_star_name(&self) -> Option<&Name> {
        self.expression.source_star_name()
    }

    pub(in crate::ast) fn projection(expression: &Expr, alias: &str) -> Self {
        Self {
            location: None,
            expression: expression.clone(),
            alias: Some(alias.to_owned()),
        }
    }
}
impl TypeName {
    pub(in crate::ast) fn projection(name: &Name, array: bool) -> Self {
        Self {
            name: name.clone(),
            modifiers: Vec::new(),
            array_bounds: if array { vec![None] } else { Vec::new() },
            set_of: false,
            percent_type: false,
            sqlite_name_snapshot: None,
            sqlite_parts: Vec::new(),
            sqlite_modifiers: None,
            location: None,
        }
    }
}
impl Cast {
    fn projection(argument: &Expr, target: &TypeName) -> Self {
        Self {
            argument: argument.clone(),
            target: target.clone(),
        }
    }
}
impl Indirection {
    fn projection_field(argument: &Expr, field: &str) -> Self {
        Self {
            argument: argument.clone(),
            items: vec![Access::projection_field(field)],
        }
    }
}
impl Call {
    fn projection_function_alias(&self) -> Option<&str> {
        self.name.projection_local_name()
    }
    pub(in crate::ast) fn coalesce_projection(arguments: &[Expr]) -> Self {
        Self {
            sql_syntax: false,
            kind: crate::ast::CallKind::special("coalesce"),
            name: Name::local("coalesce"),
            argument_locations: vec![None; arguments.len()],
            arguments: arguments.to_vec(),
            star: false,
            distinct: false,
            variadic: false,
            within_group: false,
            within_order: Vec::new(),
            order: Vec::new(),
            filter: None,
            over: None,
            location: None,
        }
    }
}
impl Expr {
    pub(in crate::ast) fn projection_function_alias(&self) -> Option<&str> {
        match self {
            Self::Call(call) => call.projection_function_alias(),
            _ => None,
        }
    }
    pub(in crate::ast) fn projection_column(name: &Name) -> Self {
        Self::Column(name.clone())
    }
    pub(in crate::ast) fn projection_call(call: &Call) -> Self {
        Self::Call(Box::new(call.clone()))
    }
    pub(in crate::ast) fn projection_cast(argument: &Self, target: &TypeName) -> Self {
        Self::Cast(Box::new(Cast::projection(argument, target)))
    }
    pub(in crate::ast) fn projection_source_row(alias: &str) -> Self {
        Self::Row(vec![Self::Star(Name::local(alias))])
    }
    pub(in crate::ast) fn projection_field(argument: &Self, field: &str) -> Self {
        Self::Indirection(Box::new(Indirection::projection_field(argument, field)))
    }
    pub(in crate::ast) const fn is_projection_star(&self) -> bool {
        matches!(self, Self::Star(_))
    }
}
impl Access {
    fn projection_field(field: &str) -> Self {
        Self::Field(field.to_owned())
    }
}
impl QueryBody {
    pub(in crate::ast) fn projection_select(select: &Select) -> Self {
        Self::Select(Box::new(select.clone()))
    }
}
impl Name {
    pub(in crate::ast) fn projection_local_name(&self) -> Option<&str> {
        self.0.last().map(String::as_str)
    }
}

impl Expr {
    pub(in crate::ast) const fn source_star_name(&self) -> Option<&Name> {
        match self {
            Self::Star(name) => Some(name),
            _ => None,
        }
    }
}
