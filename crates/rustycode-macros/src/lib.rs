#![allow(
    clippy::doc_markdown,
    clippy::needless_pass_by_value,
    clippy::redundant_closure_for_method_calls,
    clippy::uninlined_format_args
)]
#![cfg_attr(
    test,
    allow(
        clippy::cast_precision_loss,
        clippy::expect_used,
        clippy::float_cmp,
        clippy::ignored_unit_patterns,
        clippy::match_same_arms,
        clippy::redundant_clone,
        clippy::significant_drop_tightening,
        clippy::too_many_lines,
        clippy::unwrap_used,
    )
)]
//! RustyCode procedural macros for tool definitions.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, AttrStyle, Ident, ItemFn, ItemStruct, Lit, Meta, MetaNameValue};

#[proc_macro_attribute]
pub fn tool(args: TokenStream, input: TokenStream) -> TokenStream {
    let _ = args;
    let item = parse_macro_input!(input as ItemFn);

    let fn_name = &item.sig.ident;
    let fn_name_str = fn_name.to_string();

    // Generate a struct name that won't conflict with the function (append _Tool)
    let struct_name = Ident::new(&format!("{}_Tool", fn_name), Span::call_site());

    let expanded = quote! {
        #item

        #[allow(non_camel_case_types)]
        struct #struct_name;

        impl rustycode_tools::Tool for #struct_name {
            fn name(&self) -> &str {
                #fn_name_str
            }

            fn description(&self) -> &str {
                concat!(stringify!(#fn_name), " - a rustycode tool")
            }

            fn permission(&self) -> rustycode_tools::ToolPermission {
                rustycode_tools::ToolPermission::None
            }

            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                })
            }

            fn execute(&self, _params: serde_json::Value, _ctx: &rustycode_tools::ToolContext) -> Result<rustycode_tools::ToolOutput, anyhow::Error> {
                Ok(rustycode_tools::ToolOutput::text(format!("Tool {} executed", #fn_name_str)))
            }
        }
    };

    expanded.into()
}

/// Derive macro for generating tool description methods.
///
/// This derive macro adds two methods to structs:
/// - `description() -> &'static str`: Returns the tool's description from doc comments
/// - `tool_name() -> String`: Returns the struct name in snake_case
///
/// # Attributes
///
/// - `#[tool_description_file = "path/to/file.md"]`: Load description from an external file at compile time
///
/// # Examples
///
/// ```ignore
/// use rustycode_macros::ToolDescription;
///
/// #[derive(ToolDescription)]
/// /// Reads a file from the filesystem.
/// struct ReadFile;
///
/// #[derive(ToolDescription)]
/// #[tool_description_file = "docs/write_file.md"]
/// struct WriteFile;
///
/// assert_eq!(ReadFile::description(), "Reads a file from the filesystem.");
/// assert_eq!(ReadFile::tool_name(), "read_file");
/// ```
#[proc_macro_derive(ToolDescription, attributes(tool_description_file))]
pub fn tool_description(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    let struct_name = &input.ident;

    // Extract doc comments or use file attribute
    let description = extract_description(&input);

    // Convert struct name to snake_case for tool_name
    let tool_name = to_snake_case(&struct_name.to_string());
    let tool_name_lit = proc_macro2::Literal::string(&tool_name);

    let expanded = quote! {
        impl #struct_name {
            /// Returns the tool's description.
            pub fn description() -> &'static str {
                #description
            }

            /// Returns the tool name in snake_case format.
            pub fn tool_name() -> String {
                #tool_name_lit.to_string()
            }
        }
    };

    expanded.into()
}

/// Derive macro that adds Clone, Debug, Serialize, Deserialize, PartialEq, PartialOrd.
///
/// Note: This macro is DEPRECATED. Use explicit derives instead:
/// `#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]`
///
/// The derive macro protocol requires the output to be *additional* items only
/// (the compiler keeps the original struct). Re-emitting the struct causes E0428.
/// Keeping this as a no-op for backward compat.
#[proc_macro_derive(CommonTraits)]
pub fn common_traits(_input: TokenStream) -> TokenStream {
    // No-op: the struct is kept as-is by the compiler.
    // Consumers should switch to explicit derives.
    TokenStream::new()
}

/// Extracts the description from a struct's doc comments or the `tool_description_file` attribute.
fn extract_description(input: &ItemStruct) -> proc_macro2::TokenStream {
    // Check for tool_description_file attribute first
    for attr in &input.attrs {
        if attr.path().is_ident("tool_description_file") {
            if let Meta::NameValue(MetaNameValue {
                value:
                    syn::Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(lit_str),
                        ..
                    }),
                ..
            }) = &attr.meta
            {
                let path = lit_str.value();
                return quote! {
                    include_str!(#path)
                };
            }
        }
    }

    // Fall back to extracting doc comments
    let doc_comments: Vec<_> = input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc") && matches!(attr.style, AttrStyle::Outer))
        .filter_map(|attr| {
            if let Meta::NameValue(MetaNameValue {
                value:
                    syn::Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(lit_str),
                        ..
                    }),
                ..
            }) = &attr.meta
            {
                Some(lit_str.value().trim().to_string())
            } else {
                None
            }
        })
        .collect();

    if doc_comments.is_empty() {
        return quote! {
            concat!("No description provided for ", stringify!(#input.ident))
        };
    }

    // Join doc comments with newlines
    let description = doc_comments.join("\n");
    let description_lit = proc_macro2::Literal::string(&description);

    quote! {
        #description_lit
    }
}

/// Converts a CamelCase or PascalCase string to snake_case.
///
/// # Examples
///
/// The conversion is performed automatically by the `ToolDescription` derive macro:
///
/// ```
/// use rustycode_macros::ToolDescription;
///
/// #[derive(ToolDescription)]
/// /// Reads a file from the filesystem.
/// struct ReadFile;
///
/// #[derive(ToolDescription)]
/// /// Reads from a file system.
/// struct FSRead;
///
/// assert_eq!(ReadFile::tool_name(), "read_file");
/// assert_eq!(FSRead::tool_name(), "fs_read");
/// ```
fn to_snake_case(input: &str) -> String {
    // Pre-allocate result string - snake_case is typically longer than CamelCase
    let mut result = String::with_capacity(input.len() + input.len() / 4);
    let mut prev_char_was_uppercase = false;

    for (i, ch) in input.chars().enumerate() {
        if ch.is_uppercase() {
            // Add underscore before uppercase letter if:
            // - Not the first character, and
            // - Previous char was lowercase, or
            // - Next char is lowercase (acronym boundary like "FSRead" -> "fs_read")
            if i > 0
                && (!prev_char_was_uppercase
                    || input
                        .chars()
                        .nth(i + 1)
                        .is_some_and(|next| next.is_lowercase()))
            {
                result.push('_');
            }
            result.extend(ch.to_lowercase());
            prev_char_was_uppercase = true;
        } else {
            result.push(ch);
            prev_char_was_uppercase = false;
        }
    }

    result
}
