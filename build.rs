use inflector::cases::snakecase::{is_snake_case, to_snake_case};
use quote::{format_ident, quote, ToTokens};

fn main() {
    let schema_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("schemas")
        .join("subsonic-rest-api-1.16.1.xsd");
    let schema = xmltree::Element::parse(std::fs::File::open(schema_path).unwrap()).unwrap();

    let mut output = vec![];
    for node in schema.children {
        if let xmltree::XMLNode::Element(element) = node {
            match element.name.as_str() {
                "element" => {
                    // todo
                }
                "simpleType" => {
                    let name = element.attributes.get("name").unwrap();
                    match element.children.into_iter().next().unwrap() {
                        xmltree::XMLNode::Element(element) => {
                            let restriction = element.attributes.get("base").unwrap();
                            match restriction.as_str() {
                                "xs:int" => output.push(format!(
                                    "#[derive(Debug, serde::Deserialize)] pub struct {}(std::ops::RangeInclusive<i32>);",
                                    name
                                )),
                                "xs:double" => output.push(format!(
                                    "#[derive(Debug, serde::Deserialize)] pub struct {}(std::ops::RangeInclusive<f64>);",
                                    name
                                )),
                                _ => output.push(format!(r#"#[derive(Debug, serde::Deserialize)] pub struct {};"#, name)),
                            }
                        }
                        _ => panic!(),
                    }
                }
                "complexType" => {
                    if let Some(name) = element.attributes.get("name") {
                        if name == "Response" {
                            let field_data = element
                                .children
                                .into_iter()
                                .filter_map(only_elements)
                                .next()
                                .unwrap()
                                .children
                                .into_iter()
                                .filter_map(only_elements)
                                .map(|mut element| {
                                    let original_name = element.attributes.remove("name").unwrap();
                                    let xml_ty = element.attributes.remove("type").unwrap();
                                    let ty = xml_type_to_ident(&xml_ty).unwrap();
                                    let name = format_ident!(
                                        "{}",
                                        inflector::cases::pascalcase::to_pascal_case(
                                            &original_name
                                        )
                                    );
                                    (original_name, name, ty)
                                })
                                .collect::<Vec<_>>();

                            let fields = field_data.iter().map(|(original_name, name, ty)| {
                                quote! {
                                    #[serde(rename = #original_name)]
                                    #name(#ty)
                                }
                            });

                            let conversions = field_data.iter().filter_map(|(_, name, ty)| {
                                if name == "Error" {
                                    return None;
                                }
                                Some(quote! {
                                    impl From<SubsonicResponse> for crate::SubsonicResponse<#ty> {
                                        fn from(response: SubsonicResponse) -> Self {
                                            let version = response.subsonic_response.version;
                                            let result = match response.subsonic_response.content {
                                                Response::#name(inner) => Ok(inner),
                                                _ => Err(response.subsonic_response.content.into()),
                                            };
                                            Self { version, result }
                                        }
                                    }
                                })
                            });

                            output.push(
                                quote! {
                                    #[derive(Debug, serde::Deserialize)]
                                    pub enum Response {
                                        #(#fields),*
                                    }

                                    #(#conversions)*
                                }
                                .to_string(),
                            );
                            continue;
                        }

                        let fields =
                            element
                                .children
                                .into_iter()
                                .filter_map(only_elements)
                                .map(|element| match element.name.as_str() {
                                    "attribute" => {
                                        let name = element.attributes.get("name").unwrap();
                                        let ty = element.attributes.get("type").unwrap();
                                        let required =
                                            match element.attributes.get("use").unwrap().as_str() {
                                                "required" => true,
                                                "optional" => false,
                                                _ => panic!(),
                                            };

                                        let ty = if required {
                                            xml_type_to_ident(ty).unwrap().to_token_stream()
                                        } else {
                                            let ty = xml_type_to_ident(ty).unwrap();
                                            quote!(Option<#ty>)
                                        };
                                        if name == "type" {
                                            quote! {
                                                #[serde(rename = "type")]
                                                pub ty: #ty
                                            }
                                        } else if !is_snake_case(name) {
                                            let original_name = name;
                                            let name = format_ident!("{}", to_snake_case(name));
                                            quote! {
                                                #[serde(rename = #original_name)]
                                                pub #name: #ty
                                            }
                                        } else {
                                            let name = format_ident!("{}", name);
                                            quote! {
                                                pub #name: #ty
                                            }
                                        }
                                    }
                                    "sequence" => {
                                        let first = element
                                            .children
                                            .into_iter()
                                            .filter_map(only_elements)
                                            .next()
                                            .unwrap();

                                        let name = first.attributes.get("name").unwrap();
                                        let xml_ty = first.attributes.get("type").unwrap();
                                        let ty = xml_type_to_ident(xml_ty).unwrap();

                                        if name == "match" {
                                            quote! {
                                                #[serde(rename = "match")]
                                                pub matches: Vec<#ty>
                                            }
                                        } else if !is_snake_case(name) {
                                            let original_name = name;
                                            let name = format_ident!("{}", to_snake_case(name));
                                            quote! {
                                                #[serde(rename = #original_name)]
                                                pub #name: Vec<#ty>
                                            }
                                        } else {
                                            let name = format_ident!("{}", name);
                                            quote! {
                                                pub #name: Vec<#ty>
                                            }
                                        }
                                    }
                                    "complexContent" => {
                                        quote! {
                                            #[serde(flatten)]
                                            pub placeholder: serde_json::Value,
                                        }
                                    }
                                    _ => {
                                        unimplemented!("{}", element.name);
                                    }
                                });

                        let name = format_ident!("{}", name);
                        output.push(
                            quote! {
                                #[derive(Debug, serde::Deserialize)]
                                pub struct #name {
                                    #(#fields),*
                                }
                            }
                            .to_string(),
                        );
                    }
                }
                _ => unimplemented!(),
            }
        }
    }

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = std::path::Path::new(&out_dir).join("api.rs");
    std::fs::write(dest_path, output.join("\n")).unwrap();
}

fn only_elements(node: xmltree::XMLNode) -> Option<xmltree::Element> {
    if let xmltree::XMLNode::Element(element) = node {
        Some(element)
    } else {
        None
    }
}

fn xml_type_to_ident(xml: &str) -> Option<proc_macro2::Ident> {
    if xml.starts_with("sub:") {
        return Some(format_ident!("{}", xml.trim_start_matches("sub:")));
    }

    let ident = match xml {
        "xs:int" => "i32",
        "xs:string" => "String",
        "xs:long" => "i64",
        "xs:boolean" => "bool",
        "xs:dateTime" => "String",
        "xs:float" => "f32",
        "xs:double" => "f64",
        _ => {
            println!("Type Not Found: {}", xml);
            return None;
        }
    };
    Some(format_ident!("{}", ident))
}
