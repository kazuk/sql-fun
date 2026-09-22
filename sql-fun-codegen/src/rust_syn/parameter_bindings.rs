use ::proc_macro2::{Span, TokenStream};
use ::syn::{Attribute, FnArg, Ident, LitBool, LitStr, Meta, Pat, Signature, ext::IdentExt};

// Lockfile bind names are identifier-shaped. Rust's path keywords cannot be raw identifiers.
fn rust_member_ident(name: &str, span: Span) -> Ident {
    match name {
        "self" | "Self" | "super" | "crate" => Ident::new(&format!("{name}_"), span),
        _ if syn::parse_str::<Ident>(name).is_ok() => Ident::new(name, span),
        _ => Ident::new_raw(name, span),
    }
}

#[cfg(test)]
#[rstest::rstest]
#[case("name", "name")]
#[case("type", "r#type")]
#[case("match", "r#match")]
#[case("self", "self_")]
#[case("Self", "Self_")]
#[case("super", "super_")]
#[case("crate", "crate_")]
#[case("get_self", "get_self")]
fn test_rust_member_ident(#[case] name: &str, #[case] expected: &str) {
    let ident = rust_member_ident(name, Span::call_site());
    assert_eq!(ident.to_string(), expected);
    syn::parse2::<syn::Expr>(quote::quote!(dto.#ident)).unwrap();
}

struct ParameterSource {
    ident: Ident,
    getter_prefix: String,
    field_bind: bool,
}

impl ParameterSource {
    fn parse(ident: &Ident, attribute: &Attribute) -> Result<Self, syn::Error> {
        let mut getter_prefix = None;
        let mut field_bind = None;
        match &attribute.meta {
            Meta::Path(_) => (),
            Meta::List(_) => attribute.parse_nested_meta(|meta| {
                if meta.path.is_ident("getter_prefix") {
                    if getter_prefix.is_some() {
                        return Err(meta.error("duplicate getter_prefix"));
                    }
                    let value: LitStr = meta.value()?.parse()?;
                    let probe = format!("{}parameter", value.value());
                    if !syn::parse_str::<Ident>(&probe).is_ok_and(|ident| ident == probe) {
                        return Err(syn::Error::new_spanned(
                            value,
                            "getter_prefix must form a Rust identifier",
                        ));
                    }
                    getter_prefix = Some(value.value());
                } else if meta.path.is_ident("field_bind") {
                    if field_bind.is_some() {
                        return Err(meta.error("duplicate field_bind"));
                    }
                    let value: LitBool = meta.value()?.parse()?;
                    field_bind = Some(value.value);
                } else {
                    return Err(meta.error("unknown parameter_source option"));
                }
                Ok(())
            })?,
            Meta::NameValue(_) => {
                return Err(syn::Error::new_spanned(
                    attribute,
                    "expected #[parameter_source] or #[parameter_source(...)]",
                ));
            }
        }
        let field_bind = field_bind.unwrap_or(false);
        if field_bind && getter_prefix.is_some() {
            return Err(syn::Error::new_spanned(
                attribute,
                "field_bind = true and getter_prefix are mutually exclusive",
            ));
        }
        Ok(Self {
            ident: ident.clone(),
            getter_prefix: getter_prefix.unwrap_or_default(),
            field_bind,
        })
    }

    fn expression(&self, name: &Ident) -> TokenStream {
        let source = &self.ident;
        if self.field_bind {
            let member = rust_member_ident(&name.unraw().to_string(), name.span());
            quote::quote! { &#source.#member }
        } else {
            let getter_name = format!("{}{}", self.getter_prefix, name.unraw());
            let getter = rust_member_ident(&getter_name, name.span());
            quote::quote! { &#source.#getter() }
        }
    }
}

/// Rust value sources, derived from the final function signature, not from SQL.
pub(crate) struct ParameterBindings {
    direct: Vec<Ident>,
    source: Option<ParameterSource>,
}

impl TryFrom<&Signature> for ParameterBindings {
    type Error = syn::Error;

    fn try_from(signature: &Signature) -> Result<Self, Self::Error> {
        let mut bindings = Self {
            direct: Vec::new(),
            source: None,
        };
        for input in &signature.inputs {
            let FnArg::Typed(arg) = input else { continue };
            let mut source_attribute = None;
            for attribute in &arg.attrs {
                if attribute.path().is_ident("parameter_source") {
                    if source_attribute.is_some() || bindings.source.is_some() {
                        return Err(syn::Error::new_spanned(
                            attribute,
                            "only one parameter_source is supported per query function",
                        ));
                    }
                    source_attribute = Some(attribute);
                }
            }
            if let Some(attribute) = source_attribute {
                let Pat::Ident(pattern) = &*arg.pat else {
                    return Err(syn::Error::new_spanned(
                        &arg.pat,
                        "parameter_source requires an identifier argument",
                    ));
                };
                if pattern.subpat.is_some() {
                    return Err(syn::Error::new_spanned(
                        &arg.pat,
                        "parameter_source requires an identifier argument",
                    ));
                }
                bindings.source = Some(ParameterSource::parse(&pattern.ident, attribute)?);
            } else if let Pat::Ident(pattern) = &*arg.pat {
                bindings.direct.push(pattern.ident.clone());
            }
        }
        Ok(bindings)
    }
}

impl ParameterBindings {
    pub(crate) fn expression(&self, name: &str) -> TokenStream {
        if let Some(ident) = self.direct.iter().find(|ident| ident.unraw() == name) {
            return quote::quote! { &#ident };
        }
        let ident = quote::format_ident!("{}", name);
        if let Some(source) = &self.source {
            source.expression(&ident)
        } else {
            // Preserve unresolved identifiers for rustc to diagnose as before.
            quote::quote! { &#ident }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ParameterBindings;

    #[rstest::rstest]
    #[case("type", "r#type")]
    #[case("match", "r#match")]
    #[case("self", "self_")]
    #[case("Self", "Self_")]
    #[case("super", "super_")]
    #[case("crate", "crate_")]
    fn selects_keyword_members(#[case] name: &str, #[case] member: &str) {
        for (attribute, expected) in [
            ("parameter_source", format!("& dto . {member} ()")),
            (
                "parameter_source(field_bind = true)",
                format!("& dto . {member}"),
            ),
            (
                "parameter_source(getter_prefix = \"get_\")",
                format!("& dto . get_{name} ()"),
            ),
        ] {
            let signature =
                syn::parse_str::<syn::Signature>(&format!("fn query(#[{attribute}] dto: Dto)"))
                    .unwrap();
            let expression = ParameterBindings::try_from(&signature)
                .unwrap()
                .expression(name);
            assert_eq!(expression.to_string(), expected);
            syn::parse2::<syn::Expr>(expression).unwrap();
        }
    }

    #[test]
    fn preserves_direct_raw_argument_precedence() {
        let signature = syn::parse_quote!(fn query(#[parameter_source] dto: Dto, r#type: i64));
        let bindings = ParameterBindings::try_from(&signature).unwrap();
        assert_eq!(bindings.expression("type").to_string(), "& r#type");
    }

    #[rstest::rstest]
    #[case("fn query(name: &str)", "& name")]
    #[case("fn query()", "& name")]
    #[case("fn query(#[parameter_source] dto: Dto)", "& dto . name ()")]
    #[case(
        "fn query(#[parameter_source(field_bind = false)] dto: Dto)",
        "& dto . name ()"
    )]
    #[case(
        "fn query(#[parameter_source(getter_prefix = \"get_\", field_bind = false)] dto: Dto)",
        "& dto . get_name ()"
    )]
    #[case(
        "fn query(#[parameter_source(field_bind = true)] dto: Dto)",
        "& dto . name"
    )]
    #[case("fn query(#[parameter_source] dto: Dto, name: &str)", "& name")]
    #[case("fn query(#[parameter_source] name: Dto)", "& name . name ()")]
    #[case("fn query(#[parameter_source] dto: Dto, r#name: &str)", "& r#name")]
    fn selects_expression(#[case] signature: &str, #[case] expected: &str) {
        let signature = syn::parse_str::<syn::Signature>(signature).unwrap();
        let bindings = ParameterBindings::try_from(&signature).unwrap();
        assert_eq!(bindings.expression("name").to_string(), expected);
    }

    #[rstest::rstest]
    #[case(
        "#[parameter_source(field_bind = \"yes\")] dto: Dto",
        "expected boolean literal"
    )]
    #[case(
        "#[parameter_source(unknown = true)] dto: Dto",
        "unknown parameter_source option"
    )]
    #[case(
        "#[parameter_source(field_bind = true, getter_prefix = \"get_\")] dto: Dto",
        "mutually exclusive"
    )]
    #[case(
        "#[parameter_source(getter_prefix = \"get_\", field_bind = true)] dto: Dto",
        "mutually exclusive"
    )]
    #[case(
        "#[parameter_source(field_bind = true, field_bind = false)] dto: Dto",
        "duplicate field_bind"
    )]
    #[case(
        "#[parameter_source(getter_prefix = \"\", getter_prefix = \"get_\")] dto: Dto",
        "duplicate getter_prefix"
    )]
    #[case(
        "#[parameter_source(getter_prefix = true)] dto: Dto",
        "expected string literal"
    )]
    #[case(
        "#[parameter_source(getter_prefix = \"bad-\")] dto: Dto",
        "must form a Rust identifier"
    )]
    #[case("#[parameter_source = true] dto: Dto", "expected #[parameter_source]")]
    #[case(
        "#[parameter_source] #[parameter_source] dto: Dto",
        "only one parameter_source"
    )]
    #[case(
        "#[parameter_source] first: Dto, #[parameter_source] second: Dto",
        "only one parameter_source"
    )]
    #[case(
        "#[parameter_source] (first, second): (Dto, Dto)",
        "requires an identifier"
    )]
    fn rejects_invalid_attributes(#[case] inputs: &str, #[case] expected: &str) {
        let signature = syn::parse_str::<syn::Signature>(&format!("fn query({inputs})")).unwrap();
        let error = ParameterBindings::try_from(&signature)
            .err()
            .expect("invalid attribute");
        assert!(error.to_string().contains(expected), "{error}");
    }
}
