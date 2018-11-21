// Constructs a unique function identifier for Präzi
//
// (c) 2018 - onwards Joseph Hejderup <joseph.hejderup@gmail.com>
//
// MIT/APACHE licensed -- check LICENSE files in top dir
extern crate cargo;
extern crate filebuffer;
extern crate quote;
extern crate syn;
#[macro_use]
extern crate lazy_static;
extern crate regex;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate ini;

use cargo::core::resolver::Resolve;
use cargo::core::Package;
use cargo::core::Workspace;
use cargo::ops::load_pkg_lockfile;
use cargo::util::Config;
use filebuffer::FileBuffer;
use ini::Ini;
use quote::ToTokens;
use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::Path;
use std::path::PathBuf;
use std::str;
use syn::{Ident, PathSegment};

lazy_static! {
    static ref CONFIG: Ini = {
        let dir = env!("CARGO_MANIFEST_DIR");
        let conf = Ini::load_from_file(format!("{0}/{1}", dir, "conf.ini")).unwrap();
        conf
    };
    static ref PRAZI_DIR: String = {
        CONFIG
            .section(Some("storage"))
            .unwrap()
            .get("path")
            .unwrap()
            .to_string()
    };
}

fn is_a_node(text: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"(Node.*?) \[").unwrap();
    }
    RE.is_match(text)
}

fn extract_node_data(text: &str) -> Vec<&str> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r#""\{(.*?)\}""#).unwrap();
    }
    RE.captures_iter(text)
        .map(|caps| caps.get(1).map_or("", |m| m.as_str()))
        .collect::<Vec<_>>()
}

fn build_valid_rust_ident(text: &str) -> String {
    let colon2 = str::replace(text, ".", "_");
    let c = str::replace(colon2.as_str(), "-", "_");
    str::replace(c.as_str(), "+", "_")
}

fn read_ws(path: &str) -> Result<(Package, Resolve), cargo::CargoError> {
    let config = Config::default().expect("Using Default config");
    let ws = Workspace::new(Path::new(path), &config)?;
    let pkg = ws.current()?;
    let lock_file = load_pkg_lockfile(&ws)?;
    Ok((pkg.clone(), lock_file.unwrap()))
}

fn read_lib_name_from_path(path: &str) -> Result<String, cargo::CargoError> {
    let config = Config::default().expect("Should have config file");
    let ws = Workspace::new(Path::new(path), &config)?;
    let targets = ws
        .current()
        .unwrap()
        .targets()
        .into_iter()
        .filter(|target| target.is_lib() || target.is_dylib() || target.is_cdylib())
        .collect::<Vec<_>>();
    Ok(targets[0].name().to_string())
}

fn read_lib_name_from_pkg(pkg: &Package) -> Result<String, cargo::CargoError> {
    let targets = pkg
        .targets()
        .into_iter()
        .filter(|target| target.is_lib() || target.is_dylib() || target.is_cdylib())
        .collect::<Vec<_>>();
    Ok(targets[0].name().to_string())
}

#[derive(Debug, Clone)]
struct PkgIdentifier {
    pkg_name: String,
    lib_name: String,
    version: String,
}

impl PkgIdentifier {
    fn new(pkg_name: &str, lib_name: &str, version: &str) -> PkgIdentifier {
        PkgIdentifier {
            pkg_name: str::replace(pkg_name, "-", "_"),
            lib_name: str::replace(lib_name, "-", "_"),
            version: version.to_string(),
        }
    }

    fn pkg_name(&self) -> &str {
        &self.pkg_name
    }

    fn lib_name(&self) -> &str {
        &self.lib_name
    }

    fn version(&self) -> &str {
        &self.version
    }
}

fn fetch_deps(
    cargo_toml_path: &str,
) -> Result<(PkgIdentifier, Vec<Result<PkgIdentifier, cargo::CargoError>>), cargo::CargoError> {
    let (pkg, lock_file) = read_ws(cargo_toml_path)?;
    let int_lib_name = read_lib_name_from_pkg(&pkg)?;
    Ok((
        PkgIdentifier::new(
            &pkg.name(),
            int_lib_name.as_str(),
            pkg.version().to_string().as_str(),
        ),
        lock_file
            .iter()
            .map(|dep| {
                let cargo_toml_path = format!(
                    "{0}/crates/reg/{1}/{2}/Cargo.toml",
                    &**PRAZI_DIR,
                    dep.name(),
                    dep.version()
                );
                let lib_name = read_lib_name_from_path(cargo_toml_path.as_str());
                if let Err(e) = lib_name {
                    Err(e)
                } else {
                    Ok(PkgIdentifier::new(
                        &dep.name(),
                        lib_name.unwrap().as_str(),
                        dep.version().to_string().as_str(),
                    ))
                }
            }).collect::<Vec<Result<PkgIdentifier, cargo::CargoError>>>(),
    ))
}

fn is_rust_crate_ident(input: &str) -> bool {
    input == "alloc"
        || input == "core"
        || input == "proc_macro"
        || input == "std"
        || input == "std_unicode"
}

//https://github.com/rust-lang/rust/blob/5430c0c5c0fbdfb8e89358a187d2f9a8d4b796d4/src/librustc_trans/back/symbol_export.rs
fn is_rust_internal_symbol(input: &str) -> bool {
    input.starts_with("__rust_")
        || input.starts_with("__rdl_")
        || input.starts_with("rust_eh_")
        || input.starts_with("__rustc_derive_registrar")
}

fn is_llvm_symbol(input: &str) -> bool {
    input.starts_with("llvm.")
}

fn is_rust_type(input: &str) -> bool {
    input == "bool"
        || input == "u8"
        || input == "u16"
        || input == "u32"
        || input == "u64"
        || input == "i8"
        || input == "i16"
        || input == "i32"
        || input == "i64"
        || input == "binary32"
        || input == "binary64"
        || input == "f32"
        || input == "f64"
        || input == "usize"
        || input == "isize"
        || input == "char"
        || input == "String"
        || input == "str"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Symbol {
    InternalCrate,
    ExternalCrate,
    RustCrate,
    LLVMSymbol,
    RustSymbol,
    Unknown { reason: SymbolError },
    ExportedSymbol, //basically C symbol
    RustPrimitiveType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum SymbolError {
    NoDepFound,
    ParseErrorAST, //use this one for parsing into AST error
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NamespacePath {
    path: String,
    symbol: Symbol,
}

#[derive(Debug, Clone)]
struct PathVisitor<'a> {
    dependencies: &'a HashMap<String, (String, String)>,
    krate: &'a PkgIdentifier,
    namespaces: Vec<NamespacePath>,
    update_qself_pos: bool,
}

impl<'a> syn::visit_mut::VisitMut for PathVisitor<'a> {
    fn visit_expr_path_mut(&mut self, _i: &mut syn::ExprPath) {
        for it in &mut _i.attrs {
            self.visit_attribute_mut(it)
        }
        if let Some(ref mut it) = _i.qself {
            self.visit_qself_mut(it)
        };
        self.visit_path_mut(&mut _i.path);

        if let Some(ref mut qself) = _i.qself {
            //about the position https://docs.rs/syn/0.13.1/syn/struct.QSelf.html
            if self.update_qself_pos == true {
                qself.position += 4; // len(io :: crates :: pkg_name :: pkg_ver)
            }
        }
    }

    fn visit_type_path_mut(&mut self, _i: &mut syn::TypePath) {
        if let Some(ref mut it) = _i.qself {
            self.visit_qself_mut(it)
        };

        if let Some(ref mut it) = _i.qself {
            self.visit_qself_mut(it)
        };
        self.visit_path_mut(&mut _i.path);

        if let Some(ref mut qself) = _i.qself {
            //about the position https://docs.rs/syn/0.13.1/syn/struct.QSelf.html
            if self.update_qself_pos == true {
                qself.position += 4; // len(io :: crates :: pkg_name :: pkg_ver)
            }
        }
    }

    fn visit_qself_mut(&mut self, qself: &mut syn::QSelf) {
        self.visit_type_mut(&mut *qself.ty);
        //about the position https://docs.rs/syn/0.13.1/syn/struct.QSelf.html
        if self.update_qself_pos == true {
            qself.position += 4; // len(io :: crates :: pkg_name :: pkg_ver)
        }
    }

    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        let namespace_ast = path.clone();
        let first_segment = namespace_ast.segments.first().unwrap();
        let first_seg_ident = first_segment.value().ident.as_ref();
        if path.leading_colon.is_none() {
            //fn symbols that are just names and no :: (e.g namespace)
            if path.segments.len() == 1 && first_segment.punct().is_none() {
                if is_rust_internal_symbol(first_seg_ident) {
                    self.namespaces.push(NamespacePath {
                        path: path.clone().into_tokens().to_string(),
                        symbol: Symbol::RustSymbol,
                    });
                    return;
                }
                if is_llvm_symbol(first_seg_ident) {
                    self.namespaces.push(NamespacePath {
                        path: path.clone().into_tokens().to_string(),
                        symbol: Symbol::LLVMSymbol,
                    });
                    return;
                }
                if is_rust_type(first_seg_ident) {
                    self.namespaces.push(NamespacePath {
                        path: path.clone().into_tokens().to_string(),
                        symbol: Symbol::RustPrimitiveType,
                    });
                    return;
                }
                self.namespaces.push(NamespacePath {
                    path: path.clone().into_tokens().to_string(),
                    symbol: Symbol::ExportedSymbol,
                });
                return;
            } else {
                //fn symbols that have namespaces: rust crates or non-rust crates
                if is_rust_crate_ident(first_seg_ident) {
                    self.namespaces.push(NamespacePath {
                        path: path.clone().into_tokens().to_string(),
                        symbol: Symbol::RustCrate,
                    });
                } else {
                    let pkg_name = self.dependencies.get(first_seg_ident);
                    if pkg_name.is_some() {
                        self.update_qself_pos = true;
                        let id = pkg_name.unwrap();
                        path.segments.insert(
                            0,
                            PathSegment::from(Ident::from(format!(
                                "v_{}",
                                build_valid_rust_ident(id.1.as_ref())
                            ))),
                        );
                        path.segments.insert(
                            0,
                            PathSegment::from(Ident::from(build_valid_rust_ident(id.0.as_ref()))),
                        );
                        path.segments
                            .insert(0, PathSegment::from(Ident::from("crates")));
                        path.segments
                            .insert(0, PathSegment::from(Ident::from("io")));
                        if first_seg_ident == self.krate.lib_name() {
                            self.namespaces.push(NamespacePath {
                                path: format!(
                                    "io :: crates :: {} :: v_{}",
                                    build_valid_rust_ident(id.0.as_ref()),
                                    build_valid_rust_ident(id.1.as_ref())
                                ),
                                symbol: Symbol::InternalCrate,
                            });
                        } else {
                            self.namespaces.push(NamespacePath {
                                path: format!(
                                    "io :: crates :: {} :: v_{}",
                                    build_valid_rust_ident(id.0.as_ref()),
                                    build_valid_rust_ident(id.1.as_ref())
                                ),
                                symbol: Symbol::ExternalCrate,
                            });
                        }
                    } else {
                        eprintln!(
                            "({},{}):SymbolError::NoDepFound:{}:{:?}:{:?}",
                            self.krate.pkg_name(),
                            self.krate.version(),
                            path.clone().into_tokens().to_string(),
                            self.krate,
                            self.dependencies
                        );
                        self.namespaces.push(NamespacePath {
                            path: path.clone().into_tokens().to_string(),
                            symbol: Symbol::Unknown {
                                reason: SymbolError::NoDepFound,
                            },
                        });
                    }
                }
            }
        }
        //visit each segment to discover more namespaces e.g Paths in ItemImpl, etc
        for mut el in syn::punctuated::Punctuated::pairs_mut(&mut path.segments) {
            let it = el.value_mut();
            self.visit_path_segment_mut(it)
        }
    }
}

fn ufify(
    pkg: &PkgIdentifier,
    lookup: &HashMap<String, (String, String)>,
    fn_signature: &str,
) -> Option<(String, Vec<NamespacePath>)> {
    let syntax_tree = syn::parse_file(&fn_signature);
    if let Ok(mut ast) = syntax_tree {
        let mut visitor = PathVisitor {
            dependencies: lookup,
            krate: pkg,
            namespaces: Vec::new(),
            update_qself_pos: false,
        };
        // println!("before: {}", visitor.update_qself_pos);
        // println!("{:#?}", ast);
        syn::visit_mut::visit_file_mut(&mut visitor, &mut ast);
        // println!("after: {}", visitor.update_qself_pos);
        // println!("{:#?}", ast);
        Some((ast.into_tokens().to_string(), visitor.namespaces.clone()))
    } else {
        eprintln!(
            "({},{}): could not parse into AST:{}:{}",
            pkg.pkg_name(),
            pkg.version(),
            fn_signature,
            syntax_tree.unwrap_err()
        );
        None
    }
}

fn make_lookup_table(
    cargo_dir: &PathBuf,
) -> Option<(PkgIdentifier, HashMap<String, (String, String)>)> {
    let cargo_file = cargo_dir.join("Cargo.toml");
    let deps = fetch_deps(cargo_file.as_path().to_str().unwrap());
    if let Err(e) = deps {
        //This failed because we couldnt load the Cargo.toml/ws for the crate
        //Terminate here
        eprintln!("(error, no Cargo.toml):{:?}:{}", cargo_dir, e);
        None
    } else {
        let (internal, external) = deps.unwrap();
        let mut lookup: HashMap<String, (String, String)> = external
            .into_iter()
            .map(|dep| {
                if dep.is_err() {
                    None
                } else {
                    let depz = dep.unwrap();
                    Some(depz)
                }
            }).fold(HashMap::new(), |mut map, d| {
                if let Some(dep) = d {
                    map.insert(
                        dep.lib_name().to_string(),
                        (dep.pkg_name().to_string(), dep.version().to_string()),
                    );
                }
                map
            });
        lookup.insert(
            internal.lib_name().to_string(),
            (
                internal.pkg_name().to_string(),
                internal.version().to_string(),
            ),
        );
        println!(
            "({},{}): lookup table constructed with following entries: {:?}",
            internal.pkg_name(),
            internal.version(),
            lookup
        );
        Some((internal, lookup))
    }
}

fn main() {
    //
    // Create lookup table (fetching dependency data)
    //
    let base = PathBuf::from(env::args().nth(1).unwrap().as_str());
    let lookup = make_lookup_table(&base);
    if lookup.is_none() {
        //no lookup table -> exit
        return;
    }
    let (pkg, dep_table) = lookup.unwrap();
    let dot_file = base.join("callgraph.unmangled.pruned.graph");
    let fbuffer = FileBuffer::open(&dot_file);
    if let Err(e) = fbuffer {
        eprintln!(
            "({},{}): missing callgraph file or empty callgraph!: {:?}:{}",
            pkg.pkg_name(),
            pkg.version(),
            dot_file,
            e
        ); //no cg -> exit
        return;
    }
    let file_buf = fbuffer.unwrap();
    let buffer = str::from_utf8(&file_buf).expect("not valid UTF-8");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(base.join("callgraph.ufi.graph"))
        .unwrap();
    buffer.lines().for_each(|line| {
        if is_a_node(line) {
            let node_data = extract_node_data(line);
            let label = format!("fn placeholder() {{ {} }}", node_data[0]);
            let ufied_symbol = ufify(&pkg, &dep_table, &label);
            if ufied_symbol.is_none() {
                let ns = [NamespacePath {
                    path: node_data[0].to_string(),
                    symbol: Symbol::Unknown {
                        reason: SymbolError::ParseErrorAST,
                    },
                }];
                if let Err(e) = writeln!(
                    file,
                    "{},type=\"{{{}}}\"];",
                    line.split_at(line.len() - 2).0,
                    json!(ns).to_string()
                ) {
                    eprintln!("Couldn't write to file: {}", e);
                }
                return;
            }
            let (new_symbol, stats) = ufied_symbol.unwrap();
            let (ufi_1, _right_brace) = new_symbol.split_at(new_symbol.len() - 1);
            let (_fn_main, ufi_2) = ufi_1.split_at(20);
            let new_line = str::replace(&line, node_data[0], ufi_2);
            if let Err(e) = writeln!(
                file,
                "{},type=\"{{{}}}\"];",
                new_line.split_at(new_line.len() - 2).0,
                json!(stats).to_string()
            ) {
                eprintln!("Couldn't write to file: {}", e);
            }
        } else {
            if let Err(e) = writeln!(file, "{}", line) {
                eprintln!("Couldn't write to file: {}", e);
            }
        }
    });
}
