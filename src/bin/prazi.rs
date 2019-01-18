// Download package sources from crates.io, validates build manifests and construct LLVM call graphs
//
// (c) 2018 - onwards Joseph Hejderup <joseph.hejderup@gmail.com>
//
// MIT/APACHE licensed -- check LICENSE files in top dir
extern crate chrono;
extern crate clap;
extern crate crates_index;
extern crate flate2;
extern crate futures;
extern crate reqwest;
extern crate serde_json;
extern crate tar;
extern crate tokio_core;
#[macro_use]
extern crate lazy_static;
extern crate glob;
extern crate ini;
extern crate rayon;

use chrono::Utc;
use clap::{App, Arg, SubCommand};
use crates_index::Index;
use flate2::read::GzDecoder;
use futures::{stream, Future, Stream};
use glob::glob;
use ini::Ini;
use rayon::prelude::*;
use reqwest::r#async::{Client, Decoder};
use serde_json::{Error, Value};
use tar::Archive;

use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

static CRATES_ROOT: &str = "https://crates-io.s3-us-west-1.amazonaws.com/crates";

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

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub struct PraziCrate {
    pub name: String,
    pub version: String,
    pub targets: Option<Vec<Target>>,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
pub struct Target {
    name: String,
    ty: TargetType,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
enum TargetType {
    BIN,
    LIB,
}

impl PraziCrate {
    pub fn url_src(&self) -> String {
        format!(
            "{0}/{1}/{1}-{2}.crate",
            CRATES_ROOT, self.name, self.version
        )
    }

    pub fn dir(&self) -> String {
        format!(
            "{0}/crates/reg/{1}/{2}",
            &**PRAZI_DIR, self.name, self.version
        )
    }

    pub fn dir_src(&self) -> String {
        format!("{0}/crates/reg/{1}", &**PRAZI_DIR, self.name)
    }

    pub fn has_bitcode(&self) -> bool {
        let res = glob(format!("{}/target/debug/deps/*.bc", self.dir()).as_str())
            .expect("Failed to read glob pattern")
            .map(|v| v.is_ok())
            .collect::<Vec<_>>();
        res.len() == 1
    }

    pub fn bitcode_path(&self) -> PathBuf {
        let res = glob(format!("{}/target/debug/deps/*.bc", self.dir()).as_str())
            .expect("Failed to read glob pattern")
            .filter(|v| v.is_ok())
            .map(|v| v.unwrap())
            .collect::<Vec<_>>();
        res[0].to_path_buf()
    }
}

pub(crate) struct Registry {
    pub list: Vec<PraziCrate>,
}

type PraziResult<T> = std::result::Result<T, Box<std::error::Error>>;

const N: usize = 10;

impl Registry {
    fn read(&mut self) {
        let index = Index::new(format!("{}/_index", &**PRAZI_DIR));
        if !index.exists() {
            index
                .retrieve()
                .expect("Could not retrieve crates.io index");
        }
        for krate in index.crates() {
            for version in krate.versions().iter().rev() {
                //we also consider yanked versions
                self.list.push(PraziCrate {
                    name: krate.name().to_string(),
                    version: version.version().to_string(),
                    targets: None,
                });
            }
        }
    }

    fn update(&mut self) {
        let index = Index::new(format!("{}/_index", &**PRAZI_DIR));
        index.retrieve_or_update().expect("should not fail");
        for krate in index.crates() {
            for version in krate.versions().iter().rev() {
                //we also consider yanked versions
                self.list.push(PraziCrate {
                    name: krate.name().to_string(),
                    version: version.version().to_string(),
                    targets: None,
                });
            }
        }
    }

    fn read_client_file(&mut self, filename: &str) {
        let clients = fs::read_to_string(filename).expect("Something went wrong reading the file");

        let mut targets: HashMap<String, Vec<&str>> = HashMap::new();
        for client in clients.lines() {
            let mut values = client.split("\t").collect::<Vec<&str>>();
            let key = format!("{}\t{}", values[0], values[1]);

            if targets.contains_key(&key) {
                if let Some(lst) = targets.get_mut(&key) {
                    lst.push(values[2]);
                }
            } else {
                targets.insert(key, vec![values[2]]);
            }
        }

        targets.iter().for_each(|(k, v)| {
            let values = k.split("\t").collect::<Vec<&str>>();
            self.list.push(PraziCrate {
                name: values[0].to_string().replace("\"", ""),
                version: values[1].to_string().replace("\"", ""),
                targets: Some(
                    v.iter()
                        .map(|x| Target {
                            name: x.to_string().replace("\"", ""),
                            ty: TargetType::BIN,
                        }).collect(),
                ),
            });
        });
    }

    fn download_src(&self) -> PraziResult<()> {
        let mut core = tokio_core::reactor::Core::new()?;
        let client = Client::new();
        let responses = stream::iter_ok(self.list.iter().cloned())
            .map(|krate| {
                if !Path::new(&krate.dir()).exists() {
                    Some(
                        client
                            .get(&krate.url_src())
                            .send()
                            .and_then(|mut res| {
                                std::mem::replace(res.body_mut(), Decoder::empty()).concat2()
                            }).map(move |body| {
                                let mut archive = Archive::new(GzDecoder::new(body.as_ref()));
                                let tar_dir = krate.dir_src();
                                let dst_dir = krate.dir();
                                if let Err(error) = archive.unpack(&tar_dir) {
                                    eprintln!("Error unpacking: {} {:?}", tar_dir, error);
                                }
                                if let Err(error) = fs::rename(
                                    format!("/{0}/{1}-{2}", &tar_dir, krate.name, krate.version),
                                    &dst_dir,
                                ) {
                                    eprintln!("Error renaming: {} {:?}", dst_dir, error);
                                }
                            }).then(
                                |res: Result<(), reqwest::Error>| -> Result<(), reqwest::Error> {
                                    if let Err(error) = res {
                                        eprintln!("Error downloading: {:?}", error);
                                    }
                                    Ok(())
                                },
                            ),
                    )
                } else {
                    None
                }
            }).buffer_unordered(N);
        let work = responses.for_each(|_| Ok(()));
        core.run(work)?;
        Ok(())
    }

    fn validate_manifests(&self) {
        self.list.par_iter().for_each(|krate| {
            let dir = krate.dir();
            if Path::new(&dir).exists() {
                let output = Command::new("cargo")
                    .arg("read-manifest")
                    .current_dir(dir)
                    .output()
                    .expect("failed to execute read-manifest");

                if output.status.success() {
                    //  println!("Valid manifest");
                    let data = String::from_utf8_lossy(&output.stdout);
                    let v: Value = serde_json::from_str(&data).unwrap();
                    let targets = v["targets"].as_array().unwrap();
                    for target in targets.iter() {
                        for t in target["crate_types"].as_array().unwrap().iter() {
                            //    println!("crate_type: {}", t);
                        }
                    }
                } else {
                    println!("Not valid manifest");
                    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
                }
            }
        });
    }

    fn rewrite_manifests(&self) {
        self.list.par_iter().for_each(|krate| {
            let dir = krate.dir();
            if Path::new(&dir).exists() && !Path::new(&format!("{}/Cargo.toml.orig", &dir)).exists()
            {
                let output = Command::new("cargo")
                    .arg("publish")
                    .args(&["--no-verify", "--dry-run", "--allow-dirty"])
                    .current_dir(&dir)
                    .output()
                    .expect("failed to execute dry-run publish");

                if output.status.success() {
                    let new_file = format!(
                        "{0}/target/package/{1}-{2}.crate",
                        &dir, krate.name, krate.version
                    );
                    if Path::new(&new_file).exists() {
                        let data = File::open(&new_file).unwrap();
                        let decompressed = GzDecoder::new(data);
                        let mut archive = Archive::new(decompressed);
                        let tar_dir = krate.dir_src();
                        let dst_dir = krate.dir();
                        archive.unpack(&tar_dir).unwrap();
                        fs::remove_dir_all(&dst_dir).unwrap();
                        fs::rename(
                            format!("/{0}/{1}-{2}", &tar_dir, krate.name, krate.version),
                            &dst_dir,
                        ).unwrap();
                        println!("Repackaged: {:?}", &krate.url_src());
                    }
                } else {
                    println!("Package not publishable with the running Cargo version");
                }
            }
        });
    }

    fn compile_bins(&self) {
        self.list.par_iter().for_each(|krate| {
            for target in &krate.targets.clone().unwrap() {
                let bin_name = &target.name; //we skip type check (all are bins)
                if Path::new(&krate.dir()).exists()
                    && !Path::new(&format!("{}/{}.bc", krate.dir(), bin_name)).exists()
                {
                    let output = Command::new("rustup")
                        .args(&["run", "1.22.1"])
                      //  .args(&["run", "nightly-2017-12-06-x86_64-unknown-linux-gnu"])
                        .args(&["cargo", "rustc", "--bin"])
                        .arg(bin_name)
                        .args(&["--", "--emit=llvm-bc"])
                        .current_dir(krate.dir())
                        .output()
                        .expect("failed to execute cargo build");
                    if output.status.success() {
                        // println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
                        println!("{}/{}-{}: success", krate.name, krate.version, bin_name);
                        fs::rename(
                            format!("{}/Cargo.lock", krate.dir()),
                            format!("{}/{}.lock", krate.dir(), bin_name),
                        ).unwrap();
                        if krate.has_bitcode() {
                            fs::rename(
                                krate.bitcode_path(),
                                format!("{}/{}.bc.nightly", krate.dir(), bin_name),
                            ).expect(&format!(
                                "{}/{}-{}: unable to rename",
                                krate.name, krate.version, bin_name
                            ));
                        } else {
                            let timestamp = Utc::now();
                            fs::write(
                                format!("{}/{}_nobitcode", krate.dir(), bin_name),
                                format!("{}", timestamp.format("%Y-%m-%d %H:%M:%S")),
                            ).expect(&format!(
                                "{}/{}-{}: unable to write _nobitcode",
                                krate.name, krate.version, bin_name
                            ));
                        }
                    } else {
                        eprintln!(
                            "{}/{}-{}: failed :\n {}",
                            krate.name,
                            krate.version,
                            bin_name,
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                } else {
                    println!("{}/{}-{}: skip", krate.name, krate.version, bin_name);
                }
            }
        });
    }

    fn compile(&self, nightly: bool) {
        let mut rustup_args = vec!["run"];
        let version = if nightly {
            rustup_args.push("nightly");
            println!("running nightly compiler");
            CONFIG
                .section(Some("compiler"))
                .unwrap()
                .get("nightly")
                .unwrap()
        } else {
            println!("running stable compiler");
            CONFIG
                .section(Some("compiler"))
                .unwrap()
                .get("stable")
                .unwrap()
        };

        self.list.par_iter().for_each(|krate| {
            let dir = krate.dir();
            if Path::new(&dir).exists() {
                let output = Command::new("rustup")
                    .args(&rustup_args)
                    .arg(version)
                    .args(&["cargo", "rustc", "--lib", "--", "--emit=llvm-bc"])
                    .current_dir(&dir)
                    .output()
                    .expect("failed to execute cargo build");
                if output.status.success() {
                    println!("build done!");
                } else {
                    println!("build failed");
                    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
                }
            }
        });
    }

    fn build_callgraph(&self) {
        let llvm_path = CONFIG.section(Some("llvm")).unwrap().get("path").unwrap();
        self.list.par_iter().for_each(|krate| {
            let dir = krate.dir();
            if krate.has_bitcode() {
                let output = Command::new(format!("{}/bin/opt", llvm_path))
                    .current_dir(&dir)
                    .arg("-dot-callgraph")
                    .arg(krate.bitcode_path())
                    .output()
                    .expect("failed to execute llvm opt");
                if output.status.success() {
                    println!("callgraph built: {:?}", krate);
                } else {
                    println!("callgraph failed failed");
                    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
                }
            } else {
                println!("no bitcode: {:?}", krate)
            }
        });
    }
}

fn main() {
    let mut reg = Registry { list: Vec::new() };

    let matches = App::new("rustprazi")
        .version("0.1.0")
        .about("Rustpräzi: generate call-based dependency networks of crates.io registry")
        .arg(Arg::with_name("update").long("update").help("Update index"))
        .subcommand(SubCommand::with_name("download").about("download registry crate sources"))
        .subcommand(SubCommand::with_name("validate").about("validate Cargo.toml files"))
        .subcommand(
            SubCommand::with_name("read-clients")
                .arg(
                    Arg::with_name("INPUT")
                        .help("Sets the input file to use")
                        .required(true)
                        .index(1),
                ).about("read client file"),
        ).subcommand(
            SubCommand::with_name("rewrite")
                .about("rewrite Cargo.toml to remove local Path dependencies"),
        ).subcommand(
            SubCommand::with_name("build-callgraphs")
                .about("construct Crate-wide LLVM callgraphss"),
        ).subcommand(
            SubCommand::with_name("build-crates")
                .about("build all crates")
                .arg(
                    Arg::with_name("nightly")
                        .long("nightly")
                        .short("n")
                        .help("run nightly compiler"),
                ),
        ).get_matches();

    if matches.is_present("update") {
        reg.update();
        println!("Done with updating!");
    }

    if let Some(matches) = matches.subcommand_matches("download") {
        reg.read();
        reg.download_src().unwrap();
        println!("Done with downloading!");
    }

    if let Some(matches) = matches.subcommand_matches("validate") {
        reg.read();
        reg.validate_manifests();
    }

    if let Some(matches) = matches.subcommand_matches("rewrite") {
        reg.read();
        reg.rewrite_manifests();
    }

    if let Some(matches) = matches.subcommand_matches("build-callgraphs") {
        reg.read();
        reg.build_callgraph();
    }

    if let Some(matches) = matches.subcommand_matches("build-crates") {
        reg.read();
        if matches.is_present("nightly") {
            reg.compile(true);
        } else {
            reg.compile(false);
        }
    }

    if let Some(matches) = matches.subcommand_matches("read-clients") {
        let filename = matches.value_of("INPUT").unwrap();
        reg.read_client_file(filename);
      //  reg.rewrite_manifests();
        reg.compile_bins();
        println!("Done with compiling!");
    }
}
