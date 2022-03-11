use inflector::cases::snakecase::{is_snake_case, to_snake_case};
use quote::{format_ident, quote, ToTokens};

fn main() {
    let schema_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("schemas")
        .join("subsonic-rest-api-1.16.1.xsd");
    println!("cargo:rerun-if-changed=schemas/subsonic-rest-api-1.16.1.xsd");
    let schema = xmltree::Element::parse(std::fs::File::open(schema_path).unwrap()).unwrap();

    let mut output = vec![];
    for node in schema.children {
        if let xmltree::XMLNode::Element(element) = node {
            match element.name.as_str() {
                "element" => {
                    // see GenericSubsonicResponse in lib.rs
                }
                "simpleType" => {
                    let name = element.attributes.get("name").unwrap();
                    match element.children.into_iter().next().unwrap() {
                        xmltree::XMLNode::Element(element) => {
                            let restriction = element.attributes.get("base").unwrap();
                            let ty = xml_type_to_ident(restriction);
                            output.push(format!(
                                "#[derive(Debug, serde::Deserialize)] pub struct {}({});",
                                name, ty
                            ))
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
                                    let ty = xml_type_to_ident(&xml_ty);
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

                        let fields = element
                            .children
                            .into_iter()
                            .filter_map(only_elements)
                            .flat_map(|element| match element.name.as_str() {
                                "attribute" | "sequence" => vec![gen_field(element)],
                                "complexContent" => {
                                    let base = element
                                        .children
                                        .into_iter()
                                        .filter_map(only_elements)
                                        .next()
                                        .unwrap();
                                    let base_xml_ty = base.attributes.get("base").unwrap();

                                    let field = GenNamedFieldConfig {
                                        name: &String::from("base"),
                                        ty: base_xml_ty,
                                        is_required: true,
                                        is_vec: false,
                                    }
                                    .to_token_stream();

                                    let mut fields = vec![quote! {
                                        #[serde(flatten)]
                                        #field
                                    }];
                                    fields.extend(
                                        base.children
                                            .into_iter()
                                            .filter_map(only_elements)
                                            .map(gen_field),
                                    );
                                    fields
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

fn xml_type_to_ident(xml: &str) -> proc_macro2::Ident {
    if xml.starts_with("sub:") {
        return format_ident!("{}", xml.trim_start_matches("sub:"));
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
            panic!("Type Not Found: {}", xml)
        }
    };
    format_ident!("{}", ident)
}

fn accept_as_ident(ident: &str) -> bool {
    match ident {
        "_" |
        // Based on https://doc.rust-lang.org/grammar.html#keywords
        // and https://github.com/rust-lang/rfcs/blob/master/text/2421-unreservations-2018.md
        // and https://github.com/rust-lang/rfcs/blob/master/text/2420-unreserve-proc.md
        "abstract" | "as" | "become" | "box" | "break" | "const" | "continue" |
        "crate" | "do" | "else" | "enum" | "extern" | "false" | "final" | "fn" |
        "for" | "if" | "impl" | "in" | "let" | "loop" | "macro" | "match" |
        "mod" | "move" | "mut" | "override" | "priv" | "pub" | "ref" |
        "return" | "Self" | "self" | "static" | "struct" | "super" | "trait" |
        "true" | "type" | "typeof" | "unsafe" | "unsized" | "use" | "virtual" |
        "where" | "while" | "yield" => false,
        _ => true,
    }
}

fn gen_field(element: xmltree::Element) -> proc_macro2::TokenStream {
    let (element, is_vec) = if element.name == "sequence" {
        let elements = element
            .children
            .into_iter()
            .filter_map(only_elements)
            .collect::<Vec<_>>();
        let element = if elements.len() == 1 {
            elements.into_iter().next().unwrap()
        } else {
            elements.into_iter().skip(1).next().unwrap()
        };
        (element, true)
    } else {
        (element, false)
    };

    let is_required = element
        .attributes
        .get("use")
        .map(|attr| match attr.as_str() {
            "required" => true,
            "optional" => false,
            _ => panic!(),
        })
        .unwrap_or(true);

    let config = GenNamedFieldConfig {
        name: element.attributes.get("name").unwrap(),
        ty: element.attributes.get("type").unwrap(),
        is_required,
        is_vec,
    };

    config.to_token_stream()
}

struct GenNamedFieldConfig<'a> {
    name: &'a String,
    ty: &'a String,
    is_required: bool,
    is_vec: bool,
}

impl GenNamedFieldConfig<'_> {
    fn to_token_stream(self) -> proc_macro2::TokenStream {
        let ty = xml_type_to_ident(self.ty).to_token_stream();
        let ty = if self.is_vec { quote!(Vec<#ty>) } else { ty };
        let ty = if !self.is_required {
            quote!(Option<#ty>)
        } else {
            ty
        };

        if !accept_as_ident(self.name) {
            let original_name = self.name;
            let name = format_ident!("{}_subsonic", original_name);
            quote! {
                #[serde(rename = #original_name)]
                pub #name: #ty
            }
        } else if !is_snake_case(self.name) {
            let original_name = self.name;
            let name = format_ident!("{}", to_snake_case(original_name));
            quote! {
                #[serde(rename = #original_name)]
                pub #name: #ty
            }
        } else {
            let name = format_ident!("{}", self.name);
            quote! {
                pub #name: #ty
            }
        }
    }
}
