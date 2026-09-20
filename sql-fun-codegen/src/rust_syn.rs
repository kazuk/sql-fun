use proc_macro2::Span;
use quote::ToTokens;
use sql_fun_core::{CteAlias, CteCatalogReference, CteName, FragmentName};
use syn::{
    Arm, Attribute, Block, Expr, FnArg, GenericArgument, Ident, ItemFn, Lit, Pat, PatType,
    PathArguments, ReturnType, Signature, Token, Type, TypePath, parse::ParseStream,
    punctuated::Punctuated, spanned::Spanned, token::Comma,
};

use crate::SqlFunCodegenError;

pub struct FunctionArg<'a>(&'a FnArg);

impl<'a> FunctionArg<'a> {
    pub const fn new(fn_arg: &'a FnArg) -> Self {
        Self(fn_arg)
    }

    pub fn typed_ident(&self) -> Result<&Ident, SqlFunCodegenError> {
        let FnArg::Typed(typed) = self.0 else {
            return SqlFunCodegenError::require_typed_arg();
        };
        let Pat::Ident(pat_ident) = &*typed.pat else {
            return SqlFunCodegenError::require_identifier();
        };
        Ok(&pat_ident.ident)
    }
}

#[derive(Clone)]
pub struct ItemFunction(ItemFn);

impl ItemFunction {
    pub const fn new(item_fn: ItemFn) -> Self {
        Self(item_fn)
    }

    pub fn into_inner(self) -> ItemFn {
        self.0
    }

    pub const fn sig(&self) -> &Signature {
        &self.0.sig
    }

    pub fn sig_ident(&self) -> String {
        self.0.sig.ident.to_string()
    }

    pub fn set_sig_ident(&mut self, ident: Ident) {
        self.0.sig.ident = ident;
    }

    /// get argument at index
    pub fn argument(&self, index: usize) -> Option<&FnArg> {
        self.0.sig.inputs.get(index)
    }

    /// get argument name idents
    pub fn argument_name_idents(&self) -> impl Iterator<Item = &Ident> {
        self.0.sig.inputs.iter().filter_map(|f| match &f {
            FnArg::Typed(t) => match &*t.pat {
                Pat::Ident(i) => Some(&i.ident),
                _ => None,
            },
            FnArg::Receiver(_) => None,
        })
    }

    pub fn sig_span(&self) -> Span {
        self.0.sig.span()
    }

    /// get mutable body
    pub fn block_mut(&mut self) -> &mut Block {
        &mut self.0.block
    }

    pub const fn validate_async(
        &self,
        require_async: Option<bool>,
    ) -> Result<(), SqlFunCodegenError> {
        match (self.0.sig.asyncness, require_async) {
            (None, Some(false)) | (Some(_), Some(true)) | (_, None) => Ok(()),
            _ => SqlFunCodegenError::incompatible_async_mode(),
        }
    }

    pub const fn is_async(&self) -> bool {
        self.0.sig.asyncness.is_some()
    }

    /// get result type in `fn somewhat() -> result_type`
    fn return_type(&self) -> Option<TypePath> {
        let return_type = &self.0.sig.output;
        let ReturnType::Type(_, ty) = return_type.clone() else {
            return None;
        };
        let Type::Path(ty_path) = *ty else {
            return None;
        };
        Some(ty_path)
    }

    /// get function `Result::Ok` type
    pub fn result_ok_type(&self) -> Option<Type> {
        let return_type = self.return_type()?;
        let last_segment = return_type.path.segments.last()?;
        let PathArguments::AngleBracketed(args) = &last_segment.arguments else {
            return None;
        };
        let GenericArgument::Type(ty) = args.args.iter().next()? else {
            return None;
        };
        Some(ty.clone())
    }

    /// get function `Result::Err` type
    pub fn result_err_type(&self) -> Option<Type> {
        let return_type = self.return_type()?;
        let last_segment = return_type.path.segments.last()?;
        let PathArguments::AngleBracketed(args) = &last_segment.arguments else {
            return None;
        };
        let GenericArgument::Type(ty) = args.args.iter().nth(1)? else {
            return None;
        };
        Some(ty.clone())
    }

    /// find attributed argument
    fn find_attributed_argument(&self, attribute_ident: &Ident) -> Option<&PatType> {
        for input in &self.0.sig.inputs {
            let FnArg::Typed(arg) = input else { continue };
            for attr in &arg.attrs {
                if attr.path().is_ident(attribute_ident) {
                    return Some(arg);
                }
            }
        }
        None
    }

    pub fn find_attributed_argument_ident(
        &self,
        attribute_ident: &Ident,
    ) -> Result<Option<&Ident>, SqlFunCodegenError> {
        let Some(arg) = self.find_attributed_argument(attribute_ident) else {
            return Ok(None);
        };
        let Pat::Ident(pat_ident) = &*arg.pat else {
            return SqlFunCodegenError::require_identifier();
        };
        Ok(Some(&pat_ident.ident))
    }

    pub fn attrs(&self) -> &[Attribute] {
        &self.0.attrs
    }

    const SQL_FRAGMENT_ATTR: &str = "sql_fragment";

    pub fn sql_fragment_attrs(&self) -> impl Iterator<Item = &Attribute> {
        self.0
            .attrs
            .iter()
            .filter(|a| a.path().is_ident(Self::SQL_FRAGMENT_ATTR))
    }

    pub(crate) fn get_fragment_idents(&self) -> Vec<Ident> {
        self.0
            .attrs
            .iter()
            .filter_map(|a| {
                if let Ok(ident) = a.path().require_ident()
                    && ident == Self::SQL_FRAGMENT_ATTR
                {
                    Some(ident.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn get_fragment_spec_attr(
        &self,
        fragment_name: &FragmentName,
    ) -> Result<Option<SqlFragmentArgs>, SqlFunCodegenError> {
        for attr in self.sql_fragment_attrs() {
            let args: SqlFragmentArgs = attr.parse_args()?;

            if &args.fragment_name == fragment_name {
                return Ok(Some(args));
            }
        }
        Ok(None)
    }

    pub fn remove_arg_by_name(&mut self, arg_name: &Ident) {
        let iter = self
            .0
            .sig
            .inputs
            .iter()
            .filter(|&arg| {
                let FnArg::Typed(typed) = arg else {
                    return true;
                };
                let Pat::Ident(pat_ident) = &*typed.pat else {
                    return true;
                };
                &pat_ident.ident != arg_name
            })
            .cloned();
        self.0.sig.inputs = iter.collect();
    }

    pub fn append_arg(&mut self, arg: &FnArg) {
        self.0.sig.inputs.push(arg.clone());
    }
}

#[derive(Debug, Clone)]
pub struct SqlFragmentArgs {
    fragment_name: FragmentName,
    parameters: Expr,
    arms: Vec<FragmentArm>,
    consume_args: Vec<Ident>,
}

impl SqlFragmentArgs {
    pub const fn fragment_name(&self) -> &FragmentName {
        &self.fragment_name
    }

    pub fn match_arms(&self) -> &[FragmentArm] {
        &self.arms
    }

    pub const fn match_expr(&self) -> &Expr {
        &self.parameters
    }

    pub const fn consume_args(&self) -> &[Ident] {
        self.consume_args.as_slice()
    }
}

#[derive(Debug, Clone)]
pub struct FragmentArm {
    name: String,
    pat: Pat,
    guard: Option<Box<Expr>>,
    sql_fragment: String,
    introduce_args: Vec<PatType>,
    introduce_arg_idents: Vec<Ident>,
}

struct FragmentArmArgsAttr(Vec<PatType>);

impl FragmentArmArgsAttr {
    pub fn into_inner(self) -> Vec<PatType> {
        self.0
    }

    pub fn is_attr(attr: &syn::Attribute) -> bool {
        attr.meta.path().is_ident("args")
    }

    pub fn try_from_attrs(attrs: &[syn::Attribute]) -> Result<Self, syn::Error> {
        let mut result = None;

        for attr in attrs {
            if Self::is_attr(attr) {
                if result.is_some() {
                    return Err(syn::Error::new(attr.span(), "duplicate definition"));
                }
                result = Some(Self::try_from_attr(attr)?);
            }
        }

        if let Some(result) = result.flatten() {
            Ok(result)
        } else {
            Ok(Self(Vec::default()))
        }
    }

    pub fn try_from_attr(attr: &syn::Attribute) -> Result<Option<Self>, syn::Error> {
        if Self::is_attr(attr) {
            Ok(Some(Self::try_from_meta(&attr.meta)?))
        } else {
            Ok(None)
        }
    }

    pub fn try_from_meta(meta: &syn::Meta) -> Result<Self, syn::Error> {
        let list = meta.require_list()?;
        let nested = list.parse_args_with(Punctuated::<PatType, Token![,]>::parse_terminated)?;
        Ok(Self(nested.into_iter().collect()))
    }
}

impl TryFrom<Arm> for FragmentArm {
    type Error = Box<Expr>;

    fn try_from(value: Arm) -> Result<Self, Self::Error> {
        let (pat, guard) = match value.pat {
            Pat::Guard(guarded) => (*guarded.pat, Some(guarded.guard)),
            pat => (pat, None),
        };
        let source = if let Some(guard) = &guard {
            quote::quote!(#pat if #guard).to_string()
        } else {
            quote::quote!(#pat).to_string()
        };
        let name = Self::normalize_name(&source);

        let sql_fragment = value.body.as_ref();
        let Expr::Lit(sql_fragment) = sql_fragment else {
            return Err(value.body);
        };
        let Lit::Str(sql_fragment) = &sql_fragment.lit else {
            return Err(value.body);
        };
        let sql_fragment = sql_fragment.value();

        let introduce_args = FragmentArmArgsAttr::try_from_attrs(&value.attrs)
            .map_err(|_| value.body.clone())?
            .into_inner();
        let introduce_arg_idents: Result<Vec<Ident>, Self::Error> = introduce_args
            .iter()
            .map(|v| match &*v.pat {
                Pat::Ident(ident) => Ok(ident.ident.clone()),
                _ => Err(value.body.clone()),
            })
            .collect();

        Ok(Self::new(
            &name,
            pat,
            guard,
            &sql_fragment,
            &introduce_args,
            &introduce_arg_idents?,
        ))
    }
}

impl FragmentArm {
    pub fn new(
        name: &str,
        pat: Pat,
        guard: Option<Box<Expr>>,
        sql_fragment: &str,
        introduce_args: &[PatType],
        introduce_arg_idents: &[Ident],
    ) -> Self {
        Self {
            name: String::from(name),
            pat,
            guard,
            sql_fragment: String::from(sql_fragment),
            introduce_args: introduce_args.to_vec(),
            introduce_arg_idents: introduce_arg_idents.to_vec(),
        }
    }

    pub const fn pat(&self) -> &Pat {
        &self.pat
    }

    pub fn guard(&self) -> Option<&Expr> {
        self.guard.as_deref()
    }

    pub fn sql_fragment(&self) -> &str {
        &self.sql_fragment
    }

    pub fn introduce_args(&self) -> &[PatType] {
        &self.introduce_args
    }

    pub fn introduce_arg_idents(&self) -> &[Ident] {
        &self.introduce_arg_idents
    }

    pub fn normalize_name(source: &str) -> String {
        let mut result = source.to_lowercase();

        result = result.replace(|ch: char| !ch.is_ascii_alphanumeric(), "_");
        while result.contains("__") {
            result = result.replace("__", "_");
        }

        result
    }

    pub fn arm_function_name(&self, parent_fn: &ItemFunction) -> Ident {
        let base_ident = parent_fn.sig_ident();
        let pat_suffix = Self::normalize_name(&self.pat.to_token_stream().to_string());
        Ident::new(&format!("{base_ident}_{pat_suffix}"), parent_fn.sig_span())
    }
}

impl syn::parse::Parse for SqlFragmentArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        fn expect_comma(input: ParseStream<'_>) -> syn::Result<Comma> {
            input.parse::<Token![,]>()
        }

        let fragment_name: Ident = input.parse()?;
        expect_comma(input)?;

        let parameters: Expr = input.parse()?;
        expect_comma(input)?;

        eprintln!("begin parse consume_args");

        let content;
        syn::parenthesized!(content in input);
        let punct_idents = content.parse_terminated(Ident::parse, Token![,])?;
        let consume_args = punct_idents.into_iter().collect();

        expect_comma(input)?;

        let content;
        syn::braced!(content in input);

        let mut arms = Vec::new();
        while !content.is_empty() {
            let arm: Arm = content.parse()?;

            arms.push(
                FragmentArm::try_from(arm)
                    .map_err(|_e| content.error("fragment arm require string literal"))?,
            );
        }

        if !input.is_empty() {
            expect_comma(input)?;
        }

        if !input.is_empty() {
            return Err(input.error("unexpected tokens after sql_fragment arguments"));
        }

        Ok(Self {
            fragment_name: FragmentName::new(fragment_name.to_string()),
            parameters,
            consume_args,
            arms,
        })
    }
}

#[cfg(test)]
mod test_item_func_get_fragment_spec_attr {
    use assert_matches::assert_matches;
    use sql_fun_core::FragmentName;
    use testresult::TestResult;

    use crate::rust_syn::ItemFunction;

    #[rstest::rstest]
    #[case(
        r#"
            #[sql_fragment(
                fragment_name,
                parameters,
                (param1, param2, param3),
                {
                    #[args(o: &str)]
                    One(o) => "where name=${o}",
                    #[args(first: &str, second: &str)]
                    Two(first, second) => "where first=${first} and second=${second}",
                }
            )]
            fn test() {}
        "#
    )]
    fn test_get_fragment_spec(#[case] source: &str) -> TestResult {
        let item: syn::ItemFn = syn::parse_str(source)?;
        let item_fn = ItemFunction::new(item);
        let attr = item_fn.get_fragment_spec_attr(&FragmentName::new("fragment_name"))?;
        assert_matches!(attr, Some(_));
        let attr = item_fn.get_fragment_spec_attr(&FragmentName::new("non_existing_name"))?;
        assert_matches!(attr, None);
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CteReferenceAttr(CteCatalogReference);

impl CteReferenceAttr {
    pub fn new(cte_name: &str, alias: &str) -> Self {
        Self(CteCatalogReference::new(
            &CteName::new(cte_name),
            &CteAlias::new(alias),
        ))
    }

    pub const fn as_cte_reference(&self) -> &CteCatalogReference {
        &self.0
    }

    pub const fn cte_name(&self) -> &CteName {
        self.0.cte_name()
    }

    pub const fn alias(&self) -> &CteAlias {
        self.0.alias()
    }

    /// gets [`CteReference`] from [`syn::Attribute`]
    ///
    /// # Errors
    ///
    /// - [`syn::Error`]: invalid syntax
    ///
    pub fn from_attr(attr: &syn::Attribute) -> Result<Option<Self>, syn::Error> {
        if attr.meta.path().is_ident("with") {
            let arguments = attr
                .parse_args_with(Punctuated::<syn::MetaNameValue, Token![,]>::parse_terminated)?;
            let mut cte_name = None;
            let mut alias = None;
            for argument in arguments {
                let Expr::Lit(syn::ExprLit {
                    lit: Lit::Str(value),
                    ..
                }) = argument.value
                else {
                    return Err(syn::Error::new_spanned(argument, "expected string literal"));
                };
                if argument.path.is_ident("cte_name") {
                    cte_name = Some(value.value());
                } else if argument.path.is_ident("alias") {
                    alias = Some(value.value());
                } else {
                    return Err(syn::Error::new_spanned(argument.path, "unknown field"));
                }
            }
            let cte_name =
                cte_name.ok_or_else(|| syn::Error::new_spanned(attr, "missing cte_name"))?;
            let alias = alias.ok_or_else(|| syn::Error::new_spanned(attr, "missing alias"))?;
            Ok(Some(Self::new(&cte_name, &alias)))
        } else {
            Ok(None)
        }
    }

    /// gets [`Vec<CteReference>`] from [`syn::ItemFn`]
    ///
    /// # Errors
    ///
    /// - [`syn::Error`]: invalid syntax
    ///
    pub fn from_item_fn(item_fn: &ItemFunction) -> Result<Vec<Self>, syn::Error> {
        let mut results = Vec::new();
        for attr in item_fn.attrs() {
            if let Some(att) = Self::from_attr(attr)? {
                results.push(att);
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod test_cte_reference {
    use crate::rust_syn::ItemFunction;

    #[rstest::rstest]
    #[case(
        r#"
            #[sql_fun::sql_query_one(
                sql="select id, name from au where name=${id}"
            )]
            #[with(cte_name="active_users", alias="au")]
            pub async fn select_user_by_id(client: Client, id: u64) -> Result<User, tokio_postgres::Error> { }
        "#,
        "active_users",
        "au"
    )]
    fn test_parse_cte_reference(
        #[case] attr: &str,
        #[case] cte_name: &str,
        #[case] alias: &str,
    ) -> testresult::TestResult {
        let item: syn::ItemFn = syn::parse_str(attr)?;
        let item = ItemFunction::new(item);
        let cte_reference = super::CteReferenceAttr::from_item_fn(&item)?;
        assert_eq!(cte_reference[0].cte_name(), cte_name);
        assert_eq!(cte_reference[0].alias(), alias);
        Ok(())
    }
}
