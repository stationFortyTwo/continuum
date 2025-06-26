use proc_macro::TokenStream;
use quote::{quote, format_ident};
use proc_macro_crate::{crate_name, FoundCrate};
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(DODConvertible)]
pub fn derive_dod_convertible(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let crate_path = match crate_name("simulatedentity") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{}", name);
            quote!(#ident)
        }
        Err(_) => quote!(simulatedentity),
    };
    let fields = if let syn::Data::Struct(s) = input.data {
        s.fields
    } else {
        panic!("DODConvertible can only be derived for structs");
    };

    let field_infos = fields.iter().map(|f| {
        let ident = f.ident.as_ref().expect("named fields required");
        let ty = &f.ty;
        quote! {
            #crate_path::dod::FieldInfo {
                offset: ::memoffset::offset_of!(#name, #ident),
                size: ::core::mem::size_of::<#ty>(),
            }
        }
    });

    let expanded = quote! {
        impl #crate_path::dod::DODConvertible for #name {
            const FIELD_LAYOUT: &'static [#crate_path::dod::FieldInfo] = &[#(#field_infos),*];
            fn to_bytes(&self) -> Vec<u8> {
                unsafe {
                    let ptr = self as *const Self as *const u8;
                    ::core::slice::from_raw_parts(ptr, ::core::mem::size_of::<Self>()).to_vec()
                }
            }
            fn from_bytes(bytes: &[u8]) -> Self {
                assert_eq!(bytes.len(), ::core::mem::size_of::<Self>());
                unsafe { *(bytes.as_ptr() as *const Self) }
            }
        }
    };

    TokenStream::from(expanded)
}
