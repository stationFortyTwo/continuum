use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, ItemFn, FnArg, Pat, Type};

#[proc_macro_attribute]
pub fn lua_api(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let ident = &input.sig.ident;
    // collect arg patterns and types
    let mut arg_names = Vec::new();
    let mut arg_types = Vec::new();
    for arg in &input.sig.inputs {
        if let FnArg::Typed(pat) = arg {
            if let Pat::Ident(ident_pat) = &*pat.pat {
                arg_names.push(ident_pat.ident.clone());
                arg_types.push(*pat.ty.clone());
            }
        }
    }
    let register_name = format_ident!("register_{}", ident);
    let output = quote! {
        #input
        #[doc(hidden)]
        pub fn #register_name(lua: &mlua::Lua) -> mlua::Result<()> {
            let func = lua.create_function(|_, args: (#(#arg_types),*)| {
                let (#(#arg_names),*) = args;
                #ident(#(#arg_names),*);
                Ok(())
            })?;
            let globals = lua.globals();
            let game = match globals.get::<_, mlua::Table>("game") {
                Ok(t) => t,
                Err(_) => {
                    let t = lua.create_table()?;
                    globals.set("game", t.clone())?;
                    t
                }
            };
            game.set(stringify!(#ident), func)?;
            Ok(())
        }
    };
    output.into()
}
