use crate::{context::ParseContext, sem::FullName};
use std::collections::HashSet;

use super::{TypeDefinition, TypeReference};

impl TypeReference {
    /// PostgreSQL collation capability, following existing domain/array references.
    ///
    /// `None` means an undefined fact or unresolved reference (also returned for
    /// other dialects). This query does not validate SQL, report diagnostics or
    /// discover supplemental metadata. Nominal vectors use their own fact.
    pub fn collation_capability(&self, context: &impl ParseContext) -> Option<bool> {
        if context.dialect() != crate::ast::Dialect::postgres() {
            return None;
        }
        self.collation_capability_in(context, &mut HashSet::new())
    }

    fn collation_capability_in(
        &self,
        context: &impl ParseContext,
        visited: &mut HashSet<FullName>,
    ) -> Option<bool> {
        match self {
            Self::Concrete(reference) => {
                if reference.is_array() {
                    return reference
                        .element_type()
                        .collation_capability_in(context, visited);
                }
                if !visited.insert(reference.full_name().clone()) {
                    return None;
                }
                self.definition_from_catalog(context)?
                    .collation_capability_in(context, visited)
            }
            Self::Dynamic(reference) => reference
                .type_definition()
                .collation_capability_in(context, visited),
        }
    }
}

impl TypeDefinition {
    fn collation_capability_in(
        &self,
        context: &impl ParseContext,
        visited: &mut HashSet<FullName>,
    ) -> Option<bool> {
        match self {
            Self::Builtin(definition) => definition.collatable,
            Self::Array(array) => array.element().collation_capability_in(context, visited),
            Self::Domain(domain) => domain
                .base_type()?
                .collation_capability_in(context, visited),
            Self::Enum(_)
            | Self::CompositeType(_)
            | Self::TableRef(_, _)
            | Self::ImplicitComposite(_) => Some(false),
        }
    }
}

#[cfg(test)]
mod tests;
