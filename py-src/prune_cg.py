#!/usr/bin/env python
# Removing external and null nodes from an LLVM CG
#
# (c) 2017 - onwards Joseph Hejderup <joseph.hejderup@gmail.com>
#
# MIT/APACHE licensed -- check LICENSE files in top dir
import re
from log import *

###
### Regex definitions
###
re_node_pattern = re.compile(r"\W*(Node0x.*) \[.*,label=\"\{(.*)\}\"\];")
re_edge_pattern = re.compile(r"\W*(Node0x.*) -> (Node0x.*);")


def parse(dotfile):
    edges = []  # [...(Node0xSSS, Node0xDDD),(Node0xAAA, Node0xCCC),..]
    nodes = {}  # Node0x -> core::print...
    external_node = None

    with open(dotfile, "r") as dot:
        for line in dot:
            try_parse_node = re.search(re_node_pattern, line)
            if try_parse_node:
                if not nodes.has_key(try_parse_node.group(1)):
                    nodes[try_parse_node.group(1)] = try_parse_node.group(2)
                    if try_parse_node.group(2) == "external node":
                        external_node = try_parse_node.group(1)
                else:
                    info(
                        "%s already exist in the lookup table, here is the diff in value %s - %s",
                        try_parse_node.group(1),
                        nodes[try_parse_node.group(1)],
                        try_parse_node.group(2),
                    )
            else:
                try_parse_edge = re.search(re_edge_pattern, line)
                if try_parse_edge:
                    edges.append((try_parse_edge.group(1), try_parse_edge.group(2)))
    if not nodes:
        info("%s: no nodes", dotfile)
        sys.exit()
    return edges, nodes, external_node


def checkEqual(lst):
    return not lst or [lst[0]] * len(lst) == lst


def find_null_node(nodes, edges):
    """
    The null node does not have a node definition and has a node id that does not exist in the node lookup table
    """
    null_nodes = [edge[1] for edge in edges if not nodes.has_key(edge[1])]

    if not null_nodes:
        info("there are no null nodes!")
        return None
    if not checkEqual(null_nodes):
        error("there are more null nodes, should only be one!")
        sys.exit()
    return null_nodes[0]


def process(dotfolder, filename):
    edges, nodes, external_node = parse(dotfolder + "/" + filename)
    null_node = find_null_node(nodes, edges)
    pruned_edges = []
    null_nodes = []
    external_nodes = []

    # Node0xAAA -> null node, external node -> Node0xAAA
    for edge in edges:
        if (edge[1] != null_node) and (edge[0] != external_node):
            pruned_edges.append(edge)
        if edge[1] == null_node:
            null_nodes.append(edge[0])
        if edge[0] == external_node:
            external_nodes.append(edge[1])
    f = open(dotfolder + "/callgraph.unmangled.pruned.graph", "w")
    f.write('digraph "Call graph" {\n')
    for key, value in nodes.iteritems():
        if not value == "external node":
            f.write(
                "\t"
                + key
                + ' [shape=record,label="{'
                + value
                + '}",ext="{'
                + str(key in external_nodes)
                + '}",null="{'
                + str(key in null_nodes)
                + '}"];\n'
            )
    for edge in pruned_edges:
        f.write("\t" + edge[0] + " -> " + edge[1] + ";\n")
    f.write("}\n")
    f.close()


process(sys.argv[1], sys.argv[2])
