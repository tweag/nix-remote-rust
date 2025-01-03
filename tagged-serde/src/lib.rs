use proc_macro::{self, TokenStream};
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Fields, FieldsUnnamed, Ident};

#[proc_macro_derive(TaggedSerde, attributes(tagged_serde))]
pub fn derive(input: TokenStream) -> TokenStream {
    let input: DeriveInput = parse_macro_input!(input);
    let ident = input.ident;

    let syn::Data::Enum(input) = input.data else {
        // TODO: make this nice
        return quote! {
            compile_error!("not an enum");
        }
        .into();
        // panic!("not an enum");
    };

    let variants = input.variants.iter().map(|v| {
        let variant_name = &v.ident;

        let tag = v
            .attrs
            .iter()
            .find(|attr| {
                attr.meta
                    .path()
                    .get_ident()
                    .map_or(false, |i| i == "tagged_serde")
            })
            .map(|attr| {
                let nv = attr.meta.require_name_value().expect("name-value");
                &nv.value
            })
            .expect("No enum tag found for {variant_name}");

        let number_of_fields = match &v.fields {
            Fields::Unnamed(FieldsUnnamed {
                paren_token: _,
                unnamed,
            }) => Some(unnamed.len()),
            Fields::Unit => None,
            _ => unimplemented!(),
        };

        if let Some(number_of_fields) = number_of_fields {
            let field_names : Vec<_> = (0..number_of_fields).map(|n| Ident::new(&format!("field{n}"), Span::call_site())).collect();

            quote! {
                // FIXME don't hardcode u64
                #ident::#variant_name(#( #field_names ),*) => (#tag as u64, #( #field_names ),*).serialize(serializer)
            }
        } else {
            quote! {
                #ident::#variant_name => (#tag as u64).serialize(serializer)
            }
        }
    });

    let deser_variants = input.variants.iter().map(|v| {
        let variant_name = &v.ident;

        let tag = v
            .attrs
            .iter()
            .find(|attr| {
                attr.meta
                    .path()
                    .get_ident()
                    .map_or(false, |i| i == "tagged_serde")
            })
            .map(|attr| {
                let nv = attr.meta.require_name_value().expect("name-value");
                &nv.value
            })
            .expect("No enum tag found for {variant_name}");

        let number_of_fields = match &v.fields {
            Fields::Unnamed(FieldsUnnamed {
                paren_token: _,
                unnamed,
            }) => Some(unnamed.len()),
            Fields::Unit => None,
            _ => unimplemented!(),
        };

        let variant_pattern = if let Some(number_of_fields) = number_of_fields {
            let variant_args: Vec<_> = (0..number_of_fields)
                .map(|_| {
                    quote! {
                        seq
                            .next_element().map_err(|e| A::Error::custom(format!("failed to read variant with tag {}: {}", tag, e)))?
                            .ok_or_else(|| A::Error::custom(format!("failed to read variant with tag {}", tag)))?
                    }
                })
                .collect();
            quote! {
                (#( #variant_args ),*)
            }
        } else {
            quote! {
            }
        };

        quote! {
            #tag => {
                Ok(#ident::#variant_name #variant_pattern)
            }
        }
    });

    // FIXME don't hardcode u64 in the deserializer tag
    let output = quote! {
        impl ::serde::Serialize for #ident {
            fn serialize<S>(&self, serializer: S) -> ::core::result::Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                match self {
                    #( #variants ),*
                }
            }
        }

        impl<'de> Deserialize<'de> for #ident {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                use serde::de::Error;
                struct Visitor;

                impl<'d> serde::de::Visitor<'d> for Visitor {
                    type Value = #ident;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        formatter.write_str("either a string or an int")
                    }

                    fn visit_seq<A: serde::de::SeqAccess<'d>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                        let tag: u64 = seq
                            .next_element()?
                            .ok_or_else(|| A::Error::custom("failed to read logger field tag"))?;
                        match tag {
                            #( #deser_variants ),*
                            _ => Err(A::Error::custom(format!("unknown tag {} when deserializing {}", tag, stringify!(#ident)))),
                        }
                    }
                }

                // TODO: make it a tuple with 2 fields: (tag, rest)
                // We don't know yet how many fields to expect. We're abusing
                // the fact that the nix serde implementation doesn't actually
                // look at the size of the tuple.
                deserializer.deserialize_tuple(usize::MAX, Visitor)
            }
        }
    };
    output.into()
}
