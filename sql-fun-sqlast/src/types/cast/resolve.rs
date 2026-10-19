//! Dialect-aware cast resolution and candidate enumeration over loaded types.

use crate::ast::Dialect;
use crate::types::{
    Candidates, CastContext, CastDefinition, CompositeType, DomainType, EnumType,
    ImplicitCompositeType, Never, TableRowType, TypeContextRead, TypeDefinition, TypeReference,
    array::ArrayType, base::BaseType,
};

pub(in crate::types) struct CastResolver<'a, C> {
    context: &'a C,
}

impl<'a, C: TypeContextRead> CastResolver<'a, C> {
    pub(in crate::types) const fn new(context: &'a C) -> Self {
        Self { context }
    }

    pub(in crate::types) fn cast_sources(
        &self,
        target: &dyn TypeDefinition,
        explicit: bool,
    ) -> Result<TypeReference, C::Error> {
        let mode = if explicit {
            CastContext::explicit()
        } else {
            CastContext::implicit()
        };
        self.sources(target, mode)
    }

    pub(in crate::types) fn implicit_castable(
        &self,
        source: &dyn TypeDefinition,
    ) -> Result<TypeReference, C::Error> {
        let direct = self.context.implicit_castable(source)?;
        if self.context.dialect() != Dialect::postgres() {
            return Ok(direct);
        }
        let mut values = Vec::new();
        Self::append_candidates(&mut values, direct.as_ref());
        if Self::concrete_sql_type(source) {
            values.push(source.clone_box());
        }
        if let Some(array) = source.as_any().downcast_ref::<ArrayType>() {
            let elements = self.implicit_castable(array.element_type())?;
            Self::append_array_candidates(&mut values, elements.as_ref());
        } else if let Some(domain) = source.as_any().downcast_ref::<DomainType>() {
            let underlying = self.implicit_castable(domain.base_type())?;
            Self::append_candidates(&mut values, underlying.as_ref());
        }
        let mut accepted = Vec::new();
        for target in values {
            if self
                .cast(source, target.as_ref(), CastContext::implicit())?
                .is_some()
            {
                accepted.push(target);
            }
        }
        Ok(Candidates::from_candidates(&accepted))
    }

    pub(in crate::types) fn cast(
        &self,
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
        mode: CastContext,
    ) -> Result<Option<CastDefinition>, C::Error> {
        if self.context.dialect() == Dialect::postgres() {
            if source.same_type(target) && Self::concrete_sql_type(source) {
                return Ok(Some(CastDefinition::new(
                    source,
                    target,
                    &CastContext::no_conversion(),
                )));
            }
            let source_domain = source.as_any().downcast_ref::<DomainType>();
            let target_domain = target.as_any().downcast_ref::<DomainType>();
            if source_domain.is_some() || target_domain.is_some() {
                let underlying_source = source_domain.map_or(source, DomainType::base_type);
                let underlying_target = target_domain.map_or(target, DomainType::base_type);
                let Some(cast) = self.cast(underlying_source, underlying_target, mode)? else {
                    return Ok(None);
                };
                let context = if cast.context().is_no_conversion() && !source.same_type(target) {
                    CastContext::implicit()
                } else {
                    cast.context()
                };
                return Ok(Some(CastDefinition::new(source, target, &context)));
            }
            if let Some(cast) = self.context.registered_cast(source, target)? {
                return Ok(Self::eligible(cast, mode));
            }
            if let (Some(source_array), Some(target_array)) = (
                source.as_any().downcast_ref::<ArrayType>(),
                target.as_any().downcast_ref::<ArrayType>(),
            ) {
                if let Some(cast) = self.context.existing_array_cast(source, target)? {
                    return Ok(Self::eligible(cast, mode));
                }
                let Some(element_cast) = self.cast(
                    source_array.element_type(),
                    target_array.element_type(),
                    mode,
                )?
                else {
                    return Ok(None);
                };
                return Ok(Some(CastDefinition::new(
                    source,
                    target,
                    &element_cast.context(),
                )));
            }
        }
        let direct = if mode == CastContext::implicit() {
            self.context.implicit_cast(source, target)?
        } else if mode == CastContext::assignment() {
            self.context.assignment_cast(source, target)?
        } else {
            self.context.explicit_cast(source, target)?
        };
        if direct.is_some() || self.context.dialect() != Dialect::postgres() {
            return Ok(direct);
        }
        if Self::concrete_sql_type(source) && Self::concrete_sql_type(target) {
            if mode == CastContext::assignment() && Self::string_type(target) {
                return Ok(Some(CastDefinition::new(
                    source,
                    target,
                    &CastContext::assignment(),
                )));
            }
            if mode == CastContext::explicit()
                && (Self::string_type(source) || Self::string_type(target))
            {
                return Ok(Some(CastDefinition::new(
                    source,
                    target,
                    &CastContext::explicit(),
                )));
            }
        }
        Ok(None)
    }

    fn eligible(cast: CastDefinition, mode: CastContext) -> Option<CastDefinition> {
        let allowed = if mode == CastContext::implicit() {
            cast.context().use_in_implicit()
        } else if mode == CastContext::assignment() {
            cast.context().use_in_assignment()
        } else {
            cast.context().use_in_explicit()
        };
        if allowed { Some(cast) } else { None }
    }

    fn string_type(value: &dyn TypeDefinition) -> bool {
        let Some(value) = value.as_any().downcast_ref::<BaseType>() else {
            return false;
        };
        let name = value.name();
        name.catalog().is_none()
            && name
                .schema()
                .is_some_and(|schema| schema.as_str() == "pg_catalog")
            && matches!(
                name.local_name().as_str(),
                "text" | "varchar" | "bpchar" | "name"
            )
    }

    fn concrete_sql_type(value: &dyn TypeDefinition) -> bool {
        if let Some(array) = value.as_any().downcast_ref::<ArrayType>() {
            return Self::concrete_sql_type(array.element_type());
        }
        if let Some(domain) = value.as_any().downcast_ref::<DomainType>() {
            return Self::concrete_sql_type(domain.base_type());
        }
        if let Some(base) = value.as_any().downcast_ref::<BaseType>() {
            let name = base.name();
            return !(name.is_record()
                || name.is_simple_infer_arg()
                || name.is_common_infer_arg()
                || (name
                    .schema()
                    .is_some_and(|schema| schema.as_str() == "pg_catalog")
                    && matches!(
                        name.local_name().as_str(),
                        "any"
                            | "unknown"
                            | "void"
                            | "trigger"
                            | "event_trigger"
                            | "internal"
                            | "cstring"
                            | "anyenum"
                            | "language_handler"
                            | "fdw_handler"
                            | "tsm_handler"
                            | "table_am_handler"
                            | "index_am_handler"
                            | "pg_ddl_command"
                            | "opaque"
                    )));
        }
        value.as_any().is::<EnumType>()
            || value.as_any().is::<CompositeType>()
            || value.as_any().is::<ImplicitCompositeType>()
            || value.as_any().is::<TableRowType>()
    }

    pub(in crate::types) fn sources(
        &self,
        target: &dyn TypeDefinition,
        mode: CastContext,
    ) -> Result<TypeReference, C::Error> {
        let direct = if mode == CastContext::assignment() {
            self.context.assignment_sources(target)?
        } else {
            self.context
                .cast_sources(target, mode == CastContext::explicit())?
        };
        if self.context.dialect() != Dialect::postgres() {
            return Ok(direct);
        }
        let mut values = Vec::new();
        Self::append_candidates(&mut values, direct.as_ref());
        if mode == CastContext::assignment() {
            let explicit = self.context.cast_sources(target, true)?;
            Self::append_candidates(&mut values, explicit.as_ref());
        }
        if Self::concrete_sql_type(target) {
            values.push(target.clone_box());
        }
        if let Some(array) = target.as_any().downcast_ref::<ArrayType>() {
            let elements = self.sources(array.element_type(), mode)?;
            Self::append_array_candidates(&mut values, elements.as_ref());
        } else if let Some(domain) = target.as_any().downcast_ref::<DomainType>() {
            let underlying = self.sources(domain.base_type(), mode)?;
            Self::append_candidates(&mut values, underlying.as_ref());
        }
        let mut accepted = Vec::new();
        for source in values {
            if self.cast(source.as_ref(), target, mode)?.is_some() {
                accepted.push(source);
            }
        }
        Ok(Candidates::from_candidates(&accepted))
    }

    fn append_candidates(values: &mut Vec<TypeReference>, candidates: &dyn TypeDefinition) {
        if let Some(candidates) = candidates.as_any().downcast_ref::<Candidates>() {
            for candidate in candidates.candidates() {
                values.push(candidate.clone());
            }
        } else if !candidates.as_any().is::<Never>() {
            values.push(candidates.clone_box());
        }
    }

    fn append_array_candidates(values: &mut Vec<TypeReference>, candidates: &dyn TypeDefinition) {
        if let Some(candidates) = candidates.as_any().downcast_ref::<Candidates>() {
            for candidate in candidates.candidates() {
                values.push(Box::new(ArrayType::new(candidate.as_ref())));
            }
        } else if !candidates.as_any().is::<Never>() {
            values.push(Box::new(ArrayType::new(candidates)));
        }
    }
}
