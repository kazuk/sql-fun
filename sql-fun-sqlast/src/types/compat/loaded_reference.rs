//! Checked projection of complete loaded values onto legacy catalog inputs.

use super::{ConversionError, load_reference};
use crate::sem::{
    ColumnCollection, ColumnDefinition, CompositeFieldCollection, CompositeFieldDefinition,
    CreateCompositeType, ParseContext, TableName, TypeDefinition as LegacyDefinition,
    TypeReference as LegacyReference,
};
use crate::types::{
    CompositeType, DomainType, EnumType, ImplicitCompositeType, Members, TableRowType,
    TypeDefinition, array::ArrayType, base::BaseType,
};

pub(super) fn reference<C: ParseContext>(
    context: &C,
    value: &dyn TypeDefinition,
) -> Result<LegacyReference, ConversionError> {
    let reference = project(context, value, 0)?;
    let loaded = load_reference(context, &reference)?;
    if !loaded.same_type(value) {
        return Err(ConversionError::loaded_type_mismatch());
    }
    Ok(reference)
}

fn project<C: ParseContext>(
    context: &C,
    value: &dyn TypeDefinition,
    depth: usize,
) -> Result<LegacyReference, ConversionError> {
    if depth >= 128 {
        return Err(ConversionError::depth_limit());
    }
    if let Some(value) = value.as_any().downcast_ref::<BaseType>() {
        return Ok(LegacyReference::concrete_type_ref(value.name(), false));
    }
    if let Some(value) = value.as_any().downcast_ref::<EnumType>() {
        return Ok(LegacyReference::concrete_type_ref(value.name(), false));
    }
    if let Some(value) = value.as_any().downcast_ref::<DomainType>() {
        return Ok(LegacyReference::concrete_type_ref(value.name(), false));
    }
    if let Some(value) = value.as_any().downcast_ref::<TableRowType>() {
        if context.resolve_type_name(value.name()).is_some() {
            return Ok(LegacyReference::concrete_type_ref(value.name(), false));
        }
        return Ok(LegacyReference::dynamic(&LegacyDefinition::table_ref(
            &TableName::from(value.name().clone()),
        )));
    }
    if let Some(value) = value.as_any().downcast_ref::<ArrayType>() {
        let element = project(context, value.element_type(), depth + 1)?;
        if element.is_dynamic()
            && let Some(row) = value.element_type().as_any().downcast_ref::<TableRowType>()
        {
            return Ok(LegacyReference::dynamic(
                &LegacyDefinition::table_ref_with_array(&TableName::from(row.name().clone()), true),
            ));
        }
        // The legacy representation has one array flag on concrete names.
        // Anonymous or already-array elements cannot retain their identity here.
        if element.is_dynamic() || element.is_array() != Some(false) {
            return Err(ConversionError::unsupported_loaded_type(
                "array element has no scalar concrete legacy reference",
            ));
        }
        return Ok(LegacyReference::concrete_type_ref(
            element.full_name(),
            true,
        ));
    }
    if let Some(value) = value.as_any().downcast_ref::<CompositeType>() {
        if let Some(name) = value.name() {
            return Ok(LegacyReference::concrete_type_ref(name, false));
        }
        let fields = fields(context, value.members(), depth + 1)?;
        return Ok(LegacyReference::dynamic(&LegacyDefinition::from(
            &CreateCompositeType::from_fields(&fields),
        )));
    }
    if let Some(value) = value.as_any().downcast_ref::<ImplicitCompositeType>() {
        let columns = columns(context, value.members(), depth + 1)?;
        return Ok(LegacyReference::dynamic(&LegacyDefinition::from(&columns)));
    }
    Err(ConversionError::unsupported_loaded_type(
        "loaded category has no legacy catalog reference",
    ))
}

fn fields<C: ParseContext>(
    context: &C,
    members: &Members,
    depth: usize,
) -> Result<CompositeFieldCollection, ConversionError> {
    let mut fields = CompositeFieldCollection::default();
    for member in members.members() {
        let Some(name) = member.name() else {
            return Err(ConversionError::unsupported_loaded_type(
                "anonymous explicit composite member has no name",
            ));
        };
        let reference = project(context, member.member_type(), depth)?;
        fields.push(CompositeFieldDefinition::new(
            name.as_str(),
            &Some(reference),
            &member.is_not_null(),
        ));
    }
    Ok(fields)
}

fn columns<C: ParseContext>(
    context: &C,
    members: &Members,
    depth: usize,
) -> Result<ColumnCollection, ConversionError> {
    let mut columns = Vec::new();
    for member in members.members() {
        let reference = project(context, member.member_type(), depth)?;
        columns.push(ColumnDefinition::new(
            &member.name().cloned(),
            Some(&reference),
            member.is_not_null(),
            &[],
        ));
    }
    Ok(ColumnCollection::new(columns))
}
