# RustPräzi

[![Build Status](https://travis-ci.org/praezi/rust.svg?branch=master)](https://travis-ci.org/praezi/rust)

Constructing call-based dependency networks of [crates.io](https://crates.io) as conceptually described in 

>[Hejderup J, Beller M, Gousios G. Präzi: From Package-based to Precise Call-based Dependency Network Analyses. 2018.](https://pure.tudelft.nl/portal/files/46926997/main2.pdf)

# Some warnings

:warning: **DO NOT RUN IN AN UNSANDBOXED ENVIRONMENT** :warning:  

:warning: **DO NOT BUILD USING A PERSONAL MACHINE** :warning:  
Building entire crates.io (80k+ releases) requires muscle powers (cluster)
NB: building small subsets of crates.io is fully doable on a local machine 

# Getting started

## Installation Prerequisites

- The Rust toolchain with `rustup` (download at the [offical website](https://www.rust-lang.org/en-US/install.html))
- Python 2.7 or 3.7
- A pre-built binary of LLVM 4.0 (download at [official website](http://releases.llvm.org/download.html#4.0.0)). In the `config.ini` (root of the repository), specify the path to the uncompressed LLVM binary.
- Recommended OS: Ubuntu 16.04.3 LTS


## 1. Create a `conf.ini` file at the root of the project with the following content

```ini
encoding=utf-8

[llvm]
  # specify the path to the untared LLVM binary folder.
  path=/path_where/clang+llvm-4.0.0-[your_platform]

[compiler]
  stable=1.23.0
  nightly=1.24.0

[storage]
  # all data will be stored in this folder
  path=/where/you/want/to/store/prazi/data
```

Since the bitcode generation changed in newer versions of Rust, we advise to stick to the compiler versions specified above.



## 2. Constructing call graphs of crates

1. Compile the tool

``` bash
cargo build --bin prazi --release
```
2. Download crates, the downloader will fetch the latest [index](https://github.com/rust-lang/crates.io-index) data, build a list of releases and then download/untar them

```
./target/release/prazi downloader
```
3. Rewriting manifests, the manifest rewriter will fix invalid `Cargo.toml` files (e.g., specifying a non-existent local dependency) by emulating a dry-run of `cargo publish`

``` bash 
./target/release/prazi rewriter
```

4. Building crates, it will first attempt to build all downloaded crates using a stable version of the compiler (as specified in `conf.ini`). To use a nightly version for failing builds, prepend the flag `--nightly`

``` bash
./target/release/prazi build-crates
```

5. Building LLVM call graphs

``` bash
./target/release/prazi build-callgraphs
```

## 2. Construct RustPräzi

1. Install `rustfilt` for demangling of Rust symbols

```bash
cargo install rustfilt
```
2. Run graph generator script

```
./create_prazi_graph.sh 2> err.log 1> out.log
```
Two graphs are generated:
- `../cdn/graphs/callgraph.ufi.merged.graph` -- the call-based dependency network (CDN)
- `../cdn/graphs/crate.dependency.callgraph.graph` -- the packaged-based dependency network derived from the CDN

## 3. Graph analysis with RustPräzi
TODO

# License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in RustPräzi by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
