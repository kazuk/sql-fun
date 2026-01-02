# `pg_query` AST wrapper

 This module wraps [`pg_query`] AST struct and enums.

- `enum` convert to variant `as_variant_name`
- access `struct` field `get_field_name`
- AST query simplify as `as_variant().get_field().as_variant().get_field()`
  - finally `as_inner()` returns `Option<TFinalFieldValue>` 

## sturct wrapping

- `Struct` : [`std::rc::Rc`] wrapping struct value
  - expose fiels getter `get_field_name` returns field value.
- `StructOpt` : new-type wrapping `Option<Struct>`
  - expose field getter `get_field_name` returns Option value.
- `StructList` : new-type wrapping of `Vec<Struct>`
- `StructListOpt` : new-type wraping of `Oprion<Vec<Struct>>`

## enum wrapping

- `Enum` : new-type wrapping of `Enum`
  - expose convert to variant `as_variant_name` returns Option to variant.
- `EnumOpt` : new-type wrapping op `Option<Enum>`
  - expose convert to variant `as_variant_name` returns Option to variant.
- `EnumList` : new-type wrapping of `Vec<Enum>`
- `EnumListOpt` : new-type wrapping of `Option<Vec<Enum>>`

## Common `filter_map`

- `Opt` trait supports `as_inner`, `unwrap`, `ok_or_else`, `map`
- `ListOpt` trait supports `as_inner`, `get(index)`, `map`  
  - `map` works implicitly `filter_map` (None value dissmissed)
