use quote::ToTokens;
use syn::{
    FnArg, GenericArgument, GenericParam, Ident, ItemFn, Pat, Path, PathArguments, Type,
    TypeParamBound, TypePath, WherePredicate, punctuated::Punctuated,
};

pub fn get_function_output_type(item_fn: &ItemFn) -> Option<TypePath> {
    let return_type = item_fn.sig.output.clone();
    let syn::ReturnType::Type(_, ty) = return_type else {
        return None;
    };
    let syn::Type::Path(ty_path) = *ty else {
        return None;
    };
    Some(ty_path)
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

pub fn get_last_parameter_type_from_fn_trait(
    handler_ty: Punctuated<TypeParamBound, syn::token::Plus>,
) -> Option<Path> {
    handler_ty.iter().find_map(|f| {
        let TypeParamBound::Trait(tb) = f else {
            return None;
        };
        let seg = tb.path.segments.first()?;

        if seg.ident != "Fn" {
            return None;
        };
        let PathArguments::Parenthesized(pga) = &seg.arguments else {
            return None;
        };
        let Type::Path(p) = pga.inputs.last()? else {
            return None;
        };
        Some(p.path.clone())
    })
}

fn get_generic_where_bounds(
    function_ast: &ItemFn,
    name: &String,
) -> Option<Punctuated<syn::TypeParamBound, syn::token::Plus>> {
    let Some(where_clause) = &function_ast.sig.generics.where_clause else {
        return None;
    };
    where_clause.predicates.iter().find_map(|p| {
        let WherePredicate::Type(pt) = p else {
            return None;
        };
        // Check if the type name matches `name`
        if pt.bounded_ty.clone().to_token_stream().to_string() != *name {
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
    name: &String,
) -> Option<Punctuated<syn::TypeParamBound, syn::token::Plus>> {
    let handler_ty = &function_ast.sig.generics.params.iter().find_map(|p| {
        let GenericParam::Type(gp) = p else {
            return None;
        };

        if gp.ident == *name {
            if gp.bounds.is_empty() {
                None
            } else {
                Some(gp.bounds.clone())
            }
        } else {
            None
        }
    });
    handler_ty.clone()
}

pub fn get_generic_type_bounds(
    function_ast: &ItemFn,
    type_name: &String,
) -> Option<Punctuated<syn::TypeParamBound, syn::token::Plus>> {
    get_generic_param_bounds(function_ast, type_name)
        .or_else(|| get_generic_where_bounds(function_ast, type_name))
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
