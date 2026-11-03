//! The private protocol shared by connector stubs and the expanded-source reader.

use crate::{QueryFunctionSpec, SqlFunCodegenError};

use ::proc_macro2::{Span, TokenStream};
use ::sql_fun_core::ResolvedCteCatalog;
use ::syn::{Block, Ident, ItemConst, Signature, visit::Visit};

const CODEGEN_VERSION: &str = env!("CARGO_PKG_VERSION");
const VERSION: &str = "__SQL_FUN_CODEGEN_VERSION";
const SOURCE_FILE: &str = "__SQL_FUN_SOURCE_FILE";
const SOURCE_LINE: &str = "__SQL_FUN_SOURCE_LINE";
const DATASOURCE: &str = "__SQL_FUN_DATASOURCE";
const QUERY: &str = "__SQL_FUN_QUERY";
const USING_CTE: &str = "__SQL_FUN_USING_CTE";

/// A query recovered from an unresolved connector stub.
#[derive(Debug)]
pub struct QueryRequirement {
    function_name: String,
    source_file: String,
    source_line: u32,
    datasource: String,
    query: String,
    cte_catalog: ResolvedCteCatalog,
}

impl QueryRequirement {
    pub(crate) fn emit_stub_function(
        spec: &QueryFunctionSpec,
        sig: &Signature,
        source_file: &TokenStream,
    ) -> Result<TokenStream, SqlFunCodegenError> {
        let version_marker = Ident::new(VERSION, Span::call_site());
        let file_marker = Ident::new(SOURCE_FILE, Span::call_site());
        let line_marker = Ident::new(SOURCE_LINE, Span::call_site());
        let datasource_marker = Ident::new(DATASOURCE, Span::call_site());
        let query_marker = Ident::new(QUERY, Span::call_site());
        let cte_marker = Ident::new(USING_CTE, Span::call_site());
        let datasource = &spec.datasource_name;
        let query = spec.query.encode();
        let cte = serde_json::to_string(&spec.resolved_cte_catalog)?;
        Ok(quote::quote! {
            #sig {
                #[expect(dead_code)]
                const #version_marker: &str = #CODEGEN_VERSION;
                #[expect(dead_code)]
                const #file_marker: &str = #source_file;
                #[expect(dead_code)]
                const #line_marker: u32 = line!();
                #[expect(dead_code)]
                const #datasource_marker: &str = #datasource;
                #[expect(dead_code)]
                const #query_marker: &str = #query;
                #[expect(dead_code)]
                const #cte_marker: &str = #cte;
                todo!("please re run `sql-fun check`");
            }
        })
    }

    /// Name of the generated function containing this requirement.
    #[must_use]
    pub fn function_name(&self) -> &str {
        &self.function_name
    }

    /// Original source file recorded by the generator.
    #[must_use]
    pub fn source_file(&self) -> &str {
        &self.source_file
    }

    /// Original source line, after macro expansion.
    #[must_use]
    pub const fn source_line(&self) -> u32 {
        self.source_line
    }

    /// Datasource to use when checking this query.
    #[must_use]
    pub fn datasource(&self) -> &str {
        &self.datasource
    }

    /// SQL encoded by the generator, including named placeholders.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Transfer the already-resolved catalog to an analysis request.
    #[must_use]
    pub fn into_cte_catalog(self) -> ResolvedCteCatalog {
        self.cte_catalog
    }

    /// Deserialized CTE information, ready for analysis.
    #[must_use]
    pub const fn cte_catalog(&self) -> &ResolvedCteCatalog {
        &self.cte_catalog
    }
}

#[cfg(test)]
mod test_query_requirement {
    use super::QueryRequirements;
    use crate::test_helpers::query_requirements as test_helpers;

    #[test]
    fn generator_round_trip_restores_typed_cte_and_source() {
        let requirements =
            QueryRequirements::extract(&test_helpers::source(&test_helpers::expanded_body()))
                .unwrap();
        assert_eq!(requirements.len(), 1);
        let requirement = requirements.iter().next().unwrap();
        assert_eq!(requirement.function_name(), "lookup");
        assert_eq!(requirement.source_file(), "src/queries.rs");
        assert_eq!(requirement.source_line(), 42);
        assert_eq!(requirement.datasource(), "reporting");
        let spec = test_helpers::specification();
        assert_eq!(requirement.query(), spec.query.encode());
        let query = ::sql_fun_core::PlaceholderedSql::from_query(requirement.query());
        assert_eq!(
            requirement.cte_catalog().finger_print(&query).unwrap(),
            spec.resolved_cte_catalog.finger_print(&spec.query).unwrap()
        );
    }

    #[test]
    fn postgres_query_modes_emit_readable_stubs() {
        let spec = test_helpers::specification();
        for generated in [
            spec.connector_spec.render_query_one(&spec),
            spec.connector_spec.render_query_opt(&spec),
            spec.connector_spec.render_query_many(&spec),
            spec.connector_spec.render_execute(&spec),
        ] {
            let function = test_helpers::expand_location(&generated.unwrap());
            let requirements = QueryRequirements::extract(&syn::parse_quote!(#function)).unwrap();
            assert_eq!(requirements.len(), 1);
        }
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn rusqlite_query_modes_emit_readable_stubs() {
        let spec = test_helpers::sqlite_specification();
        for generated in [
            spec.connector_spec.render_query_one(&spec),
            spec.connector_spec.render_query_opt(&spec),
            spec.connector_spec.render_query_many(&spec),
            spec.connector_spec.render_execute(&spec),
        ] {
            let function = test_helpers::expand_location(&generated.unwrap());
            let requirements = QueryRequirements::extract(&syn::parse_quote!(#function)).unwrap();
            assert_eq!(requirements.len(), 1);
            let requirement = requirements.iter().next().unwrap();
            assert_eq!(requirement.datasource(), "reporting");
            assert_eq!(requirement.query(), spec.query.encode());
            let query = ::sql_fun_core::PlaceholderedSql::from_query(requirement.query());
            assert_eq!(
                requirement.cte_catalog().finger_print(&query).unwrap(),
                spec.resolved_cte_catalog.finger_print(&spec.query).unwrap()
            );
        }
    }
}

/// Query requirements found in expanded Rust source, in traversal order.
#[derive(Debug, Default)]
pub struct QueryRequirements(Vec<QueryRequirement>);

impl QueryRequirements {
    /// Extract unresolved stubs at any depth using normal `syn` traversal.
    ///
    /// Only a function's direct version marker identifies a stub. Its format is
    /// private to this crate, and its codegen version must match exactly.
    ///
    /// # Errors
    /// Returns an error for mismatched versions or malformed identified stubs,
    /// including invalid serialized CTE data. No partial result is returned.
    pub fn extract(file: &syn::File) -> Result<Self, SqlFunCodegenError> {
        let mut visitor = RequirementVisitor::default();
        visitor.visit_file(file);
        if let Some(error) = visitor.error {
            return Err(error);
        }
        Ok(visitor.requirements)
    }

    /// Number of unresolved queries.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the expanded source contains no unresolved queries.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Visit the extracted requirements.
    pub fn iter(&self) -> impl Iterator<Item = &QueryRequirement> {
        self.0.iter()
    }
}

impl IntoIterator for QueryRequirements {
    type Item = QueryRequirement;
    type IntoIter = std::vec::IntoIter<QueryRequirement>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod test_query_requirements {
    use super::QueryRequirements;
    use crate::test_helpers::query_requirements as test_helpers;

    #[test]
    fn separate_stubs_keep_their_own_fields() {
        let first = test_helpers::expanded_body();
        let mut second = first.clone();
        test_helpers::replace_marker(
            &mut second,
            super::DATASOURCE,
            quote::quote!(
                const __SQL_FUN_DATASOURCE: &str = "second";
            ),
        );
        let requirements = QueryRequirements::extract(&syn::parse_quote! {
            fn first() #first
            fn second() #second
        })
        .unwrap();
        let datasources: Vec<_> = requirements
            .iter()
            .map(super::QueryRequirement::datasource)
            .collect();
        assert_eq!(datasources, ["reporting", "second"]);
    }

    #[test]
    fn nested_fields_cannot_complete_a_stub_and_partial_results_are_not_returned() {
        let valid = test_helpers::expanded_body();
        let mut invalid = valid.clone();
        test_helpers::replace_marker(&mut invalid, super::DATASOURCE, quote::quote!());
        invalid.stmts.push(syn::parse_quote!({
            const __SQL_FUN_DATASOURCE: &str = "nested";
        }));
        let file = syn::parse_quote! { fn valid() #valid fn invalid() #invalid };
        assert!(
            QueryRequirements::extract(&file)
                .unwrap_err()
                .to_string()
                .contains("missing __SQL_FUN_DATASOURCE")
        );
    }
}

struct StubConstants<'a> {
    function: &'a Ident,
    constants: Vec<&'a ItemConst>,
}

impl<'a> StubConstants<'a> {
    fn new(function: &'a Ident, block: &'a Block) -> Self {
        let mut constants = Vec::new();
        for statement in &block.stmts {
            if let syn::Stmt::Item(syn::Item::Const(item)) = statement {
                constants.push(item);
            }
        }
        Self {
            function,
            constants,
        }
    }

    fn is_stub(&self) -> bool {
        self.constants.iter().any(|item| item.ident == VERSION)
    }

    fn error(&self, message: impl std::fmt::Display) -> SqlFunCodegenError {
        syn::Error::new_spanned(
            self.function,
            format!("invalid sql-fun stub `{}`: {message}", self.function),
        )
        .into()
    }

    fn required(&self, name: &str) -> Result<&ItemConst, SqlFunCodegenError> {
        let mut matches = self.constants.iter().filter(|item| item.ident == name);
        let Some(item) = matches.next() else {
            return Err(self.error(format!("missing {name}")));
        };
        if matches.next().is_some() {
            return Err(self.error(format!("duplicate {name}")));
        }
        Ok(item)
    }

    fn string(&self, name: &str) -> Result<String, SqlFunCodegenError> {
        let item = self.required(name)?;
        let expected_type: syn::Type = syn::parse_quote!(&str);
        if *item.ty != expected_type {
            return Err(self.error(format!("{name} must have type &str")));
        }
        if let syn::Expr::Lit(expression) = &*item.expr
            && let syn::Lit::Str(value) = &expression.lit
        {
            return Ok(value.value());
        }
        Err(self.error(format!("{name} must be a string literal")))
    }

    fn line(&self) -> Result<u32, SqlFunCodegenError> {
        let item = self.required(SOURCE_LINE)?;
        let expected_type: syn::Type = syn::parse_quote!(u32);
        if *item.ty != expected_type {
            return Err(self.error(format!("{SOURCE_LINE} must have type u32")));
        }
        if let syn::Expr::Lit(expression) = &*item.expr
            && let syn::Lit::Int(value) = &expression.lit
            && (value.suffix().is_empty() || value.suffix() == "u32")
        {
            return value.base10_parse().map_err(|error| self.error(error));
        }
        Err(self.error(format!("{SOURCE_LINE} must be an expanded u32 literal")))
    }

    fn read(&self) -> Result<QueryRequirement, SqlFunCodegenError> {
        let version = self.string(VERSION)?;
        if version != CODEGEN_VERSION {
            return Err(SqlFunCodegenError::codegen_version_mismatch(
                &self.function.to_string(),
                CODEGEN_VERSION,
                &version,
            ));
        }
        let source_file = self.string(SOURCE_FILE)?;
        let source_line = self.line()?;
        let datasource = self.string(DATASOURCE)?;
        let query = self.string(QUERY)?;
        let cte_json = self.string(USING_CTE)?;
        let cte_catalog = serde_json::from_str(&cte_json)
            .map_err(|error| self.error(format!("invalid {USING_CTE}: {error}")))?;
        Ok(QueryRequirement {
            function_name: self.function.to_string(),
            source_file,
            source_line,
            datasource,
            query,
            cte_catalog,
        })
    }
}

#[cfg(test)]
mod test_stub_constants {
    use super::{CODEGEN_VERSION, QueryRequirements};
    use crate::test_helpers::query_requirements as test_helpers;

    #[test]
    fn missing_and_duplicate_fields_are_errors_after_identification() {
        for name in [
            super::SOURCE_FILE,
            super::SOURCE_LINE,
            super::DATASOURCE,
            super::QUERY,
            super::USING_CTE,
        ] {
            let mut body = test_helpers::expanded_body();
            test_helpers::replace_marker(&mut body, name, quote::quote!());
            let error = QueryRequirements::extract(&test_helpers::source(&body)).unwrap_err();
            assert!(
                error.to_string().contains(&format!("missing {name}")),
                "{error}"
            );
        }
        for name in [
            super::VERSION,
            super::SOURCE_FILE,
            super::SOURCE_LINE,
            super::DATASOURCE,
            super::QUERY,
            super::USING_CTE,
        ] {
            let mut body = test_helpers::expanded_body();
            let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            body.stmts
                .push(syn::parse_quote!(const #ident: &str = "duplicate";));
            let error = QueryRequirements::extract(&test_helpers::source(&body)).unwrap_err();
            assert!(
                error.to_string().contains(&format!("duplicate {name}")),
                "{error}"
            );
        }
    }

    #[test]
    fn invalid_marker_types_values_and_cte_data_are_errors() {
        for (name, replacement) in [
            (
                super::VERSION,
                quote::quote!(
                    const __SQL_FUN_CODEGEN_VERSION: u32 = 1;
                ),
            ),
            (
                super::QUERY,
                quote::quote!(
                    const __SQL_FUN_QUERY: bool = "query";
                ),
            ),
            (
                super::QUERY,
                quote::quote!(
                    const __SQL_FUN_QUERY: &str = concat!("query");
                ),
            ),
            (
                super::SOURCE_LINE,
                quote::quote!(
                    const __SQL_FUN_SOURCE_LINE: &str = "42";
                ),
            ),
            (
                super::SOURCE_LINE,
                quote::quote!(
                    const __SQL_FUN_SOURCE_LINE: u32 = 4294967296;
                ),
            ),
            (
                super::SOURCE_LINE,
                quote::quote!(
                    const __SQL_FUN_SOURCE_LINE: u32 = 1u64;
                ),
            ),
            (
                super::SOURCE_LINE,
                quote::quote!(
                    const __SQL_FUN_SOURCE_LINE: u32 = line!();
                ),
            ),
            (
                super::USING_CTE,
                quote::quote!(
                    const __SQL_FUN_USING_CTE: &str = "not JSON";
                ),
            ),
            (
                super::USING_CTE,
                quote::quote!(
                    const __SQL_FUN_USING_CTE: &str = "{}";
                ),
            ),
        ] {
            let mut body = test_helpers::expanded_body();
            test_helpers::replace_marker(&mut body, name, replacement);
            assert!(
                QueryRequirements::extract(&test_helpers::source(&body)).is_err(),
                "{name}"
            );
        }
    }

    #[test]
    fn version_is_checked_exactly_before_decoding_payload() {
        for version in ["0.0.0", "999.0.0", &format!("{CODEGEN_VERSION}+other")] {
            let file = syn::parse_quote! {
                fn incompatible() { const __SQL_FUN_CODEGEN_VERSION: &str = #version; }
            };
            let error = QueryRequirements::extract(&file).unwrap_err().to_string();
            assert!(error.contains("version mismatch"), "{error}");
            assert!(error.contains(CODEGEN_VERSION), "{error}");
            assert!(error.contains(version), "{error}");
            assert!(error.contains("incompatible"), "{error}");
        }
    }
}

#[derive(Default)]
struct RequirementVisitor {
    requirements: QueryRequirements,
    error: Option<SqlFunCodegenError>,
}

impl RequirementVisitor {
    // A recognized stub cannot contain another generated stub. Stop descending
    // after reading it, or after any error, but recurse into ordinary functions.
    fn inspect(&mut self, name: &Ident, block: &Block) -> bool {
        if self.error.is_some() {
            return true;
        }
        let constants = StubConstants::new(name, block);
        if !constants.is_stub() {
            return false;
        }
        match constants.read() {
            Ok(requirement) => self.requirements.0.push(requirement),
            Err(error) => self.error = Some(error),
        }
        true
    }
}

impl<'ast> Visit<'ast> for RequirementVisitor {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !self.inspect(&item.sig.ident, &item.block) {
            syn::visit::visit_item_fn(self, item);
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !self.inspect(&item.sig.ident, &item.block) {
            syn::visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if let Some(block) = &item.default
            && self.inspect(&item.sig.ident, block)
        {
            return;
        }
        syn::visit::visit_trait_item_fn(self, item);
    }
}

#[cfg(test)]
mod test_requirement_visitor {
    use super::{CODEGEN_VERSION, QueryRequirements};
    use crate::test_helpers::query_requirements as test_helpers;

    #[test]
    fn traverses_all_function_kinds_and_nested_blocks() {
        let body = test_helpers::expanded_body();
        let file = syn::parse_quote! {
            mod outer { mod inner { fn module_query() #body } }
            fn free_query() #body
            impl Queries {
                fn method_query(&self) #body
                fn ordinary_method(&self) { fn inside_method() #body }
            }
            trait QueriesTrait {
                fn declaration();
                fn default_query(&self) #body
                fn ordinary_default(&self) { fn inside_default() #body }
            }
            fn outer_function() {
                fn nested_query() #body
                let closure = || { if true { fn block_query() #body } };
                const VALUE: () = { fn const_query() #body };
            }
        };
        let requirements = QueryRequirements::extract(&file).unwrap();
        let names: Vec<_> = requirements
            .iter()
            .map(super::QueryRequirement::function_name)
            .collect();
        assert_eq!(
            names,
            [
                "module_query",
                "free_query",
                "method_query",
                "inside_method",
                "default_query",
                "inside_default",
                "nested_query",
                "block_query",
                "const_query"
            ]
        );
    }

    #[test]
    fn partial_markers_without_version_do_not_identify_a_stub() {
        let body = test_helpers::expanded_body();
        let file = syn::parse_quote! {
            fn ordinary() {
                const __SQL_FUN_QUERY: bool = false;
                const __SQL_FUN_USING_CTE: &str = "invalid JSON";
                fn actual_stub() #body
            }
            fn resolved() {
                const __SQL_FUN_SOURCE_FILE: &str = "src/resolved.rs";
                const __SQL_FUN_SOURCE_LINE: u32 = 1;
            }
            fn __sql_fun_query_info() {
                const SQL_TEXT: &str = "old protocol";
            }
        };
        let requirements = QueryRequirements::extract(&file).unwrap();
        assert_eq!(requirements.len(), 1);
        assert_eq!(
            requirements.iter().next().unwrap().function_name(),
            "actual_stub"
        );
    }

    #[test]
    fn block_marker_does_not_identify_the_enclosing_function() {
        let version = CODEGEN_VERSION;
        let file = syn::parse_quote! {
            fn ordinary() { { const __SQL_FUN_CODEGEN_VERSION: &str = #version; } }
        };
        assert!(QueryRequirements::extract(&file).unwrap().is_empty());
    }
}
