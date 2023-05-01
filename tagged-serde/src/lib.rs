use proc_macro::{self, TokenStream};
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(TaggedSerde, attributes(tagged_serde))]
pub fn derive(input: TokenStream) -> TokenStream {
    let input: DeriveInput = parse_macro_input!(input);
    let ident = input.ident;

    let syn::Data::Enum(input) = input.data else {
        panic!("not an enum");
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

        quote! {
            // FIXME don't hardcode u64
            #ident::#variant_name(arg) => (#tag as u64, arg).serialize(serializer)
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

        quote! {
            #tag => {
                let val = seq
                    .next_element()?
                    .ok_or_else(|| A::Error::custom("failed to read logger field int"))?;
                Ok(#ident::#variant_name(val))
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
                            _ => Err(A::Error::custom("unknown logger field type")),
                        }
                    }
                }

                deserializer.deserialize_tuple(2, Visitor)
            }
        }
    };
    output.into()
}

// #[actor(msg = Blah, handle = Foo)]
//  #[serde_tag = blah]

/*
struct TaggedEnum {
    tags: BTreeMap<String, u64>,
}

impl Parse for TaggedEnum {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let original = input.parse::<ItemEnum>()?;

        Ok(TaggedEnum {
            tags: BTreeMap::new(),
        })
    }
}

struct VariantLabel {
    tag: u64,
}

impl Parse for VariantLabel {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let key = input.parse::<Ident>()?;
        if key != "tag" {
            return Err(syn::Error::new_spanned(key, format!("expected \"tag\"")));
        }
        input.parse::<Token![=]>()?;
        let value = input.parse::<LitInt>()?;

        Ok(VariantLabel {
            tag: value.base10_parse()?,
        })
    }
}
*/
