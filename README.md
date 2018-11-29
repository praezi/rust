# RustPräzi

[![Build Status](https://travis-ci.org/praezi/rust.svg?branch=master)](https://travis-ci.org/praezi/rust)
[![LOC](https://tokei.rs/b1/github/praezi/rust)](https://github.com/praezi/rust)
[![Join the chat at https://gitter.im/praezi/rust](https://badges.gitter.im/praezi/rust.svg)](https://gitter.im/praezi/rust?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge&utm_content=badge)

Constructing call-based dependency networks of [crates.io](https://crates.io) as conceptually described in 

>[Hejderup J, Beller M, Gousios G. Präzi: From Package-based to Precise Call-based Dependency Network Analyses. 2018.](https://pure.tudelft.nl/portal/files/46926997/main2.pdf)

## TL;DR: What does RustPräzi do?

### Description

With RustPräzi, we go from coarse-grained package-based dependency networks (such as what GitHub uses for their [vulnerable package detection](https://help.github.com/articles/about-security-alerts-for-vulnerable-dependencies/)) to more fine-grained call-based dependency networks. These allow us to track, for example, whether a vulnerable function of a library is actually being used and whether a security warning really needs to be raised. This is much more precise than package-based dependency networks. In fact, RustPräzi makes such analyses a lot more precise (upto 3x).

![Package-based (PDN, above) versus Call-based Dependency Networks (CDN, below)](doc/pdn_cdn.png "Package-based (PDN, above) versus Call-based Dependency Networks (CDN, below)")

### Use cases

RustPräzi opens the door to many new or more precise analyses:

* Fine-grained security vulnerability propagation checking
* Precise license compliance checking 
* Change impact and deprecation analysis ("Which clients break if I as a library maintainer remove this deprecated method?")
* Health analyses of an entire ecosystem ("What are the most central functions?", "Where should we focus our testing efforts?", ...)
* ... and more!

## Getting started

### Installation Prerequisites

- The Rust toolchain with `rustup` (download at the [offical website](https://www.rust-lang.org/en-US/install.html))
- Python 2.7 or 3.7
- GNU Parallel
- A pre-built binary of LLVM 4.0 (download at [official website](http://releases.llvm.org/download.html#4.0.0)). In the `config.ini` (root of the repository), specify the path to the uncompressed LLVM binary.
- Recommended OS: Ubuntu 16.04.3 LTS

### System Setup
- :warning: Building crates can be dangerous as for some crates, this includes running the tests. Hence, it is advised to do it in a sandboxed environment.
- 💻 We recommend running it on a very powerful system. Compiling 80k+ crates is no easy feat.


### 1. Create a `conf.ini` file at the root of the project with the following content

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



### 2. Constructing call graphs of crates

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

### 2. Construct RustPräzi

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

### 3. Graph analysis with RustPräzi




<details>

<summary>
Loading Präzi with <a href="https://networkx.github.io">NetworkX</a>
</summary>

``` python
import networkx as nx
import re

regex = r"^(.*?) \[label:"

def load_prazi(file):
    PRAZI = nx.DiGraph()
    with open(file) as f: #callgraph.ufi.merged.graph
        for line in f:
            if "->" not in line:
                g = re.match(regex, line)
                if g:
                     PRAZI.add_node(g.group(1).strip('"'))
                else:
                    print "error, could not extract node: %s" % line
            else:
                g = re.match('\W*"(.*)" -> "(.*)";', line)
                if g:
                     PRAZI.add_edge(g.group(1), g.group(2))
                else:
                    print "error, could not extract edge: %s" % line
    return PRAZI

def load_prazi_dep(file): 
    PRAZI_DEP = nx.DiGraph()
    with open(file) as f: #crate.dependency.callgraph.graph
        for line in f:
            if "io :: crates :: " in line:
                if "->" not in line:
                     PRAZI_DEP.add_node(line[:-2])
                else:
                    g = re.match('\W*"(.*)" -> "(.*)";', line)
                    if g and ("io :: crates" in g.group(1) and "io :: crates" in g.group(2)):
                         PRAZI_DEP.add_edge(g.group(1), g.group(2))
                    else:
                        print "skip edge: %s" % line
            else:
                continue
    return  PRAZI_DEP

```
</details>

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
