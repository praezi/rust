#!/usr/bin/env python
# Reads-in an LLVM opt-generated, Präzi-annotated "dot file" and outputs it such that it can easily be merged
#
# (c) 2018 - onwards Moritz Beller <moritz beller @ gmx de>
#
# MIT/APACHE licensed -- check LICENSE files in top dir
import sys
import re
import pprint
import os.path

node_to_label = {}
lines = []


def escape_label(label):
    return '"' + label + '"'


file = sys.argv[1]
if not os.path.exists(file):
    print >> sys.stderr, file + " does not exist!"
    exit(1)

with open(file) as f:
    for line in f:
        # example for a Node in Präzi-syntax: Node0x580c640 [shape=record,label="{ core :: num :: < impl usize > :: wrapping_mul }",ext="{False}",null="{False}",type="{[{"path":"core :: num :: < impl usize > :: wrapping_mul","symbol":"RustCrate"},{"path":"usize","symbol":"RustPrimitiveType"}]}"];
        m = re.match('\W*(Node0x.*) \[(.*),label="\{ (.*?) \}"(.*)];', line)
        if m:
            node_to_label[m.group(1)] = m.group(3)
            print escape_label(m.group(3)) + " [" + m.group(2) + m.group(4) + "];"
        else:
            no_wspace = re.match('\W*(Node0x.*) \[(.*),label="\{(.*?)\}"(.*)];', line)
            if no_wspace:
                node_to_label[no_wspace.group(1)] = no_wspace.group(3)
                print escape_label(no_wspace.group(3)) + " [" + no_wspace.group(
                    2
                ) + no_wspace.group(4) + "];"
            else:
                lines.append(line.rstrip("\n"))

for line in lines:
    g = re.match("\W*(Node0x.*) -> (Node0x.*);", line)
    if g:
        label1 = node_to_label[g.group(1)]
        label2 = node_to_label[g.group(2)]
        print escape_label(label1) + " -> " + escape_label(label2) + ";"
