CFG_FILE=conf.ini
read_storage_config=($(awk -F '=' -v input="storage" '$1 ~ input{flag=1; next} $1 ~ /\[object/{flag=0; next} flag && NF {split($0,arr,"="); print arr[2] }' $CFG_FILE ))
PRAZI_DIR="${read_storage_config[0]}/crates/reg"
UFI_DIR="${read_storage_config[0]}/cdn/graphs"
BASH_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null && pwd )"
PYSRC_DIR="$BASH_DIR/py-src"
UFI_BIN="$BASH_DIR/target/release/ufi"

cargo build --bin ufi --release
mkdir -p $UFI_DIR
cd $PRAZI_DIR
export LD_LIBRARY_PATH=$(rustc --print sysroot)/lib:$LD_LIBRARY_PATH
ls -d */* | parallel 'if [ -f {}/callgraph.dot ];
 			then rm {}/callgraph.unmangled.graph;
                        rustfilt -i {}/callgraph.dot -o {}/callgraph.unmangled.graph; 
                        python $PYSRC_DIR/prune_cg.py $PRAZI_DIR/{} callgraph.unmangled.graph;
                        $UFI_BIN $PRAZI_DIR/{};
                        python $PYSRC_DIR/prepare_unified_callgraphs.py $PRAZI_DIR/{}/callgraph.ufi.graph > $PRAZI_DIR/{}/callgraph.ufi.prepared.graph; fi' 
rm $UFI_DIR/callgraph.ufi.notmerged.graph
find . -name "callgraph.ufi.prepared.graph" -print0 | parallel -j1 -0 "cat {} >> $UFI_DIR/callgraph.ufi.notmerged.graph"
python $PYSRC_DIR/merge_unified_callgraphs.py $UFI_DIR/callgraph.ufi.notmerged.graph 1> $UFI_DIR/callgraph.ufi.merged.graph 2> $UFI_DIR/callgraph.ufi.merged.graph.log
python $PYSRC_DIR/infer_dependency_network_from_callgraphs.py $UFI_DIR/callgraph.ufi.merged.graph 1> $UFI_DIR/crate.dependency.callgraph.graph 2> $UFI_DIR/crate.dependency.callgraph.graph.log