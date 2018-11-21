// Download package sources from crates.io, validates build manifests and construct LLVM call graphs
//
// (c) 2018 - onwards Joseph Hejderup <joseph.hejderup@gmail.com>
//
// MIT/APACHE licensed -- check LICENSE files in top dir
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

use crates_index::Index;
use flate2::read::GzDecoder;
use futures::{stream, Future, Stream};
use glob::glob;
use ini::Ini;
use rayon::prelude::*;
use reqwest::r#async::{Client, Decoder};
use serde_json::{Error, Value};
use tar::Archive;

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

type Result<T> = std::result::Result<T, Box<std::error::Error>>;

const N: usize = 5;

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
                });
            }
        }
    }

    fn download_src(&self) -> Result<()> {
        let mut core = tokio_core::reactor::Core::new()?;
        let client = Client::new();
        let responses = stream::iter_ok(self.list.iter().cloned())
            .map(|krate| {
                client
                    .get(&krate.url_src())
                    .send()
                    .and_then(|mut res| {
                        std::mem::replace(res.body_mut(), Decoder::empty()).concat2()
                    }).map(move |body| {
                        let mut archive = Archive::new(GzDecoder::new(body.as_ref()));
                        let tar_dir = krate.dir_src();
                        let dst_dir = krate.dir();
                        archive.unpack(&tar_dir).unwrap();
                        fs::rename(
                            format!("/{0}/{1}-{2}", &tar_dir, krate.name, krate.version),
                            &dst_dir,
                        ).unwrap();
                        println!("Untared: {:?}", &krate.url_src());
                    })
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

    fn compile(&self, nightly: bool) {
        let mut rustup_args = vec!["run"];
        let version = if nightly {
            rustup_args.push("nightly");
            CONFIG
                .section(Some("compiler"))
                .unwrap()
                .get("nightly")
                .unwrap()
        } else {
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
    reg.read();
    println!("Done with the index!");
    // reg.download_src().unwrap();
    println!("Done with downloading!");
    // reg.validate_manifests();
    //reg.rewrite_manifests();
    // reg.compile(false);
    reg.build_callgraph();
}
