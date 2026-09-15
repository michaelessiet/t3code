use proc_macro::TokenStream;

/// No-op `#[instrument]`: returns the annotated item unchanged, discarding
/// any attribute arguments (`skip_all`, `fields(...)`, etc.).
#[proc_macro_attribute]
pub fn instrument(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
