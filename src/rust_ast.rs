use ::quote::ToTokens;
use ::syn::{
    FnArg, GenericArgument, GenericParam, Ident, ItemFn, Pat, Path, PathArguments, ReturnType,
    Type, TypeParamBound, TypePath, WherePredicate, punctuated::Punctuated,
};

pub fn get_function_output_type(item_fn: &ItemFn) -> Option<TypePath> {
    let return_type = item_fn.sig.output.clone();
    let ReturnType::Type(_, ty) = return_type else {
        return None;
    };
    let Type::Path(ty_path) = *ty else {
        return None;
    };
    Some(ty_path)
}

#[cfg(test)]
mod test_get_function_output_type {
    use super::*;
    use ::quote::ToTokens;

    #[test]
    fn handles_none_and_non_path() {
        let no_return: ItemFn = ::syn::parse_quote! { fn no_return() {} };
        assert!(get_function_output_type(&no_return).is_none());

        let tuple_return: ItemFn = ::syn::parse_quote! { fn tuple_return() -> (u32, u32) {} };
        assert!(get_function_output_type(&tuple_return).is_none());
    }

    #[test]
    fn extracts_path() {
        let item_fn: ItemFn = ::syn::parse_quote! { fn ok() -> Result<u32, MyError> {} };
        let ty_path = get_function_output_type(&item_fn).expect("expected type path");
        assert_eq!(
            ty_path.to_token_stream().to_string(),
            "Result < u32 , MyError >"
        );
    }
}
/// returns T of Result<T,E>
pub fn get_result_type_value(type_path: &TypePath) -> Option<Type> {
    let last_segment = type_path.path.segments.last()?;
    let PathArguments::AngleBracketed(args) = &last_segment.arguments else {
        return None;
    };
    let GenericArgument::Type(ty) = args.args.iter().next()? else {
        return None;
    };
    Some(ty.clone())
}

/// returns T of Result<T,E>
pub fn get_result_type_error(type_path: &TypePath) -> Option<Type> {
    let last_segment = type_path.path.segments.last()?;
    let PathArguments::AngleBracketed(args) = &last_segment.arguments else {
        return None;
    };
    let GenericArgument::Type(ty) = args.args.iter().nth(1)? else {
        return None;
    };
    Some(ty.clone())
}

/// returns (T,E) for `fn somewhat() -> Result<T,E> ` syntax
pub fn get_result_types(item_fn: &ItemFn) -> Option<(Type, Type)> {
    let type_path = get_function_output_type(item_fn)?;
    let value_type = get_result_type_value(&type_path)?;
    let error_type = get_result_type_error(&type_path)?;
    Some((value_type, error_type))
}

#[cfg(test)]
mod test_get_result_types {
    use super::{
        get_function_output_type, get_result_type_error, get_result_type_value, get_result_types,
    };
    use ::quote::ToTokens;
    use ::syn::ItemFn;

    #[test]
    fn result_type_helpers_extract_value_and_error() {
        let item_fn: ItemFn = ::syn::parse_quote! { fn ok() -> Result<u32, MyError> {} };
        let type_path = get_function_output_type(&item_fn).expect("expected type path");

        let value = get_result_type_value(&type_path).expect("value type");
        let error = get_result_type_error(&type_path).expect("error type");

        assert_eq!(value.to_token_stream().to_string(), "u32");
        assert_eq!(error.to_token_stream().to_string(), "MyError");
    }

    #[test]
    fn result_types_extracts_both() {
        let item_fn: ItemFn = ::syn::parse_quote! { fn ok() -> Result<u8, MyError> {} };
        let (value, error) = get_result_types(&item_fn).expect("result types");
        assert_eq!(value.to_token_stream().to_string(), "u8");
        assert_eq!(error.to_token_stream().to_string(), "MyError");
    }
}

pub fn get_last_parameter_type_from_fn_trait(
    handler_ty: &Punctuated<TypeParamBound, syn::token::Plus>,
) -> Option<Path> {
    handler_ty.iter().find_map(|f| {
        let TypeParamBound::Trait(tb) = f else {
            return None;
        };
        let seg = tb.path.segments.first()?;

        if seg.ident != "Fn" {
            return None;
        }
        let PathArguments::Parenthesized(pga) = &seg.arguments else {
            return None;
        };
        let Type::Path(p) = pga.inputs.last()? else {
            return None;
        };
        Some(p.path.clone())
    })
}

#[cfg(test)]
mod test_get_last_parameter_type_from_fn_trait {
    use super::get_last_parameter_type_from_fn_trait;
    use ::quote::ToTokens;
    use ::syn::{TypeParamBound, punctuated::Punctuated};

    #[test]
    fn picks_last_input() {
        let bounds: Punctuated<TypeParamBound, syn::token::Plus> =
            ::syn::parse_quote!(Fn(i32, MyType) -> bool + Send);
        let path = get_last_parameter_type_from_fn_trait(&bounds).expect("expected path");
        assert_eq!(path.to_token_stream().to_string(), "MyType");
    }

    #[test]
    fn ignores_non_fn() {
        let bounds: Punctuated<TypeParamBound, syn::token::Plus> = ::syn::parse_quote!(Send + Sync);
        assert!(get_last_parameter_type_from_fn_trait(&bounds).is_none());
    }

    #[test]
    fn requires_path_type() {
        let bounds: Punctuated<TypeParamBound, syn::token::Plus> =
            ::syn::parse_quote!(Fn(i32, (u32, u32)) + Send);
        assert!(get_last_parameter_type_from_fn_trait(&bounds).is_none());
    }
}

fn get_generic_where_bounds(
    function_ast: &ItemFn,
    name: &str,
) -> Option<Punctuated<syn::TypeParamBound, syn::token::Plus>> {
    let Some(where_clause) = &function_ast.sig.generics.where_clause else {
        return None;
    };
    where_clause.predicates.iter().find_map(|p| {
        let WherePredicate::Type(pt) = p else {
            return None;
        };
        // Check if the type name matches `name`
        if pt.bounded_ty.to_token_stream().to_string() != name {
            return None;
        }
        if pt.bounds.is_empty() {
            return None;
        }
        Some(pt.bounds.clone())
    })
}

fn get_generic_param_bounds(
    function_ast: &ItemFn,
    name: &str,
) -> Option<Punctuated<syn::TypeParamBound, syn::token::Plus>> {
    function_ast.sig.generics.params.iter().find_map(|p| {
        let GenericParam::Type(gp) = p else {
            return None;
        };

        if gp.ident == name {
            if gp.bounds.is_empty() {
                None
            } else {
                Some(gp.bounds.clone())
            }
        } else {
            None
        }
    })
}

pub fn get_generic_type_bounds(
    function_ast: &ItemFn,
    type_name: &str,
) -> Option<Punctuated<syn::TypeParamBound, syn::token::Plus>> {
    get_generic_param_bounds(function_ast, type_name)
        .or_else(|| get_generic_where_bounds(function_ast, type_name))
}

#[cfg(test)]
mod test_get_generic_type_bounds {
    use super::get_generic_type_bounds;
    use ::quote::ToTokens;
    use ::syn::ItemFn;

    #[test]
    fn prefers_param_bounds() {
        let item_fn: ItemFn = ::syn::parse_quote! { fn ok<T: Clone + Send>() {} };
        let bounds = get_generic_type_bounds(&item_fn, "T").expect("expected bounds");
        let bounds_str = bounds.to_token_stream().to_string();
        assert!(bounds_str.contains("Clone"));
        assert!(bounds_str.contains("Send"));
    }

    #[test]
    fn falls_back_to_where_clause() {
        let item_fn: ItemFn = ::syn::parse_quote! { fn ok<T>() where T: Copy + Debug {} };
        let bounds = get_generic_type_bounds(&item_fn, "T").expect("expected bounds");
        let bounds_str = bounds.to_token_stream().to_string();
        assert!(bounds_str.contains("Copy"));
        assert!(bounds_str.contains("Debug"));
    }
}

pub fn get_last_parameter_type(function_ast: &ItemFn) -> Option<Ident> {
    let last_arg = function_ast.sig.inputs.last()?;
    let FnArg::Typed(last_arg_type) = last_arg else {
        return None;
    };
    let Type::Path(ref ty_path) = *last_arg_type.ty else {
        return None;
    };
    Some(ty_path.path.segments[0].ident.clone())
}

pub fn get_last_parameter_name(function_ast: &ItemFn) -> Option<Ident> {
    function_ast.sig.inputs.last().and_then(get_arg_name)
}

pub fn get_arg_name(fn_arg: &FnArg) -> Option<Ident> {
    let FnArg::Typed(arg_typed) = fn_arg else {
        return None;
    };
    let Pat::Ident(ref arg_ident) = *arg_typed.pat else {
        return None;
    };
    Some(arg_ident.ident.clone())
}

#[cfg(test)]
mod test_last_parameter_helpers {
    use ::syn::ItemFn;

    use super::{get_arg_name, get_last_parameter_name, get_last_parameter_type};

    #[test]
    fn last_parameter_type_and_name() {
        let item_fn: ItemFn = ::syn::parse_quote! { fn ok(a: i32, handler: Handler) {} };
        let ty = get_last_parameter_type(&item_fn).expect("type");
        let name = get_last_parameter_name(&item_fn).expect("name");
        assert_eq!(ty.to_string(), "Handler");
        assert_eq!(name.to_string(), "handler");
    }

    #[test]
    fn last_parameter_type_returns_none_for_receiver() {
        let item_fn: ItemFn = ::syn::parse_quote! { fn ok(&self) {} };
        assert!(get_last_parameter_type(&item_fn).is_none());
    }

    #[test]
    fn get_arg_name_handles_non_ident_pattern() {
        let item_fn: ItemFn = ::syn::parse_quote! { fn ok((a, b): (i32, i32)) {} };
        let arg = item_fn.sig.inputs.first().expect("arg");
        assert!(get_arg_name(arg).is_none());
    }
}
