//! Lexical relation references used for recursive WITH dependency analysis.

use std::collections::HashMap;

use crate::ast::{
    Cte, CteBody, Dialect, FromItem, Query, QueryBody, SetOperation, SetQuery, With,
    traversal::{AstVisitorMut, RecursionRestriction, WalkSqlMut},
};

pub(crate) fn cte_key(name: &str, dialect: Dialect) -> String {
    if dialect == Dialect::sqlite() {
        name.to_ascii_lowercase()
    } else {
        name.to_owned()
    }
}

struct References {
    dialect: Dialect,
    scopes: Vec<(bool, Vec<String>)>,
    references: HashMap<String, Vec<bool>>,
    restrictions: usize,
}

impl AstVisitorMut for References {
    fn enter_with(&mut self, with: &With) {
        let names = if with.recursive() {
            with.entries()
                .iter()
                .map(|entry| cte_key(entry.name(), self.dialect))
                .collect()
        } else {
            Vec::new()
        };
        self.scopes.push((with.recursive(), names));
    }

    fn leave_with(&mut self, _: &With) {
        self.scopes.pop();
    }

    fn enter_recursive_restriction(&mut self, restriction: RecursionRestriction) {
        self.restrictions += usize::from(self.restricts(restriction));
    }

    fn leave_recursive_restriction(&mut self, restriction: RecursionRestriction) {
        self.restrictions -= usize::from(self.restricts(restriction));
    }

    fn leave_cte(&mut self, cte: &mut Cte) {
        if let Some((false, names)) = self.scopes.last_mut() {
            names.push(cte_key(cte.name(), self.dialect));
        }
    }

    fn source(&mut self, source: &mut FromItem) {
        if let FromItem::Relation(relation) = source
            && let [name] = relation.name().parts()
        {
            let name = cte_key(name, self.dialect);
            if !self.scopes.iter().any(|(_, names)| names.contains(&name)) {
                self.references
                    .entry(name)
                    .or_default()
                    .push(self.restrictions == 0);
            }
        }
    }
}

impl References {
    fn restricts(&self, restriction: RecursionRestriction) -> bool {
        match restriction {
            RecursionRestriction::Subquery => true,
            RecursionRestriction::Derived => self.dialect == Dialect::sqlite(),
            RecursionRestriction::NullableJoin | RecursionRestriction::SetOperand => {
                self.dialect == Dialect::postgres()
            }
        }
    }
}

impl CteBody {
    pub(crate) fn cte_references(&self, dialect: Dialect) -> HashMap<String, Vec<bool>> {
        let mut references = References {
            dialect,
            scopes: Vec::new(),
            references: HashMap::new(),
            restrictions: 0,
        };
        self.clone().walk_sql_mut(&mut references);
        references.references
    }
}

impl CteBody {
    pub(crate) fn recursive_union(
        &self,
        name: &str,
        dialect: Dialect,
    ) -> Option<(&Query, &SetQuery)> {
        let Self::Query(query) = self else {
            return None;
        };
        let QueryBody::Set(set) = query.body() else {
            return None;
        };
        if !matches!(set.operation(), SetOperation::Union)
            || (dialect == Dialect::postgres()
                && (!query.order().is_empty()
                    || query.limit().is_some()
                    || query.offset().is_some()
                    || !query.locks().is_empty()))
        {
            return None;
        }
        let name = cte_key(name, dialect);
        let mut references = References {
            dialect,
            scopes: Vec::new(),
            references: HashMap::new(),
            restrictions: 0,
        };
        let whole = self.cte_references(dialect);
        if whole.get(&name).map(Vec::len) != Some(1) {
            return None;
        }
        references.enter_with(query.with());
        query.with().clone().walk_sql_mut(&mut references);
        if references.references.remove(&name).is_some() {
            return None;
        }
        set.right().clone().walk_sql_mut(&mut references);
        if references.references.get(&name).map(Vec::as_slice) != Some(&[true]) {
            return None;
        }
        Some((query, set))
    }
}
