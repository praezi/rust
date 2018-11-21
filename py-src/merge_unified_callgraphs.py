# coding=utf-8
#!/usr/bin/env python
# Reads-in a non-merged call graph file in Präzi syntax and outputs a merged call graph, with statistical information
# annotated
#
# (c) 2018 - onwards Moritz Beller <moritz beller @ gmx de>
#
# MIT/APACHE licensed -- check LICENSE files in top dir

import sys
import re
import os.path

nodes = {}
edges = set()


def escape_label(label):
    return '"' + label + '"'


class CGFunction:
    """A simple data holder class for function nodes"""

    def __init__(self):
        # Number of nodes that are folded in this node, that had an internal crate type
        self.nodes_with_internal_crate = 0
        # Number of nodes that are folded in this node, that did not have an internal crate type
        self.nodes_without_internal_crate = 0
        # Number of nodes that are folded in this node, that are not null
        self.nodes_not_null = 0
        # Number of nodes that are folded in this node, that are null
        self.nodes_null = 0
        # Number of nodes that are folded in this node, that were flagged as external
        self.nodes_external = 0
        # Total number of nodes that are folded in this node (must be at least 1). Any number greater than 1 means nodes
        # have been merged
        self.nodes = 0
        self.label = ""
        self.type = ""

    def __str__(self):
        classMembers = vars(self)
        return (
            escape_label(self.label)
            + " ["
            + ", ".join(
                '%s: "%s"' % (item, str(classMembers[item]))
                for item in sorted(classMembers)
            )
            + "];"
        )


file = sys.argv[1]
if not os.path.exists(file):
    print >> sys.stderr, file + " does not exist!"
    exit(1)

with open(file) as f:
    for line in f:
        # example for a Node in Präzi-syntax: "{ core :: num :: < impl usize > :: wrapping_mul }" [shape=record,label="{ core :: num :: < impl usize > :: wrapping_mul }",ext="{False}",null="{False}",type="{[{"path":"core :: num :: < impl usize > :: wrapping_mul","symbol":"RustCrate"},{"path":"usize","symbol":"RustPrimitiveType"}]}"];
        m = re.match('\W*"(.*?)" \[(.*)\];', line)
        if m:
            node_name = m.group(1)
            attributes = m.group(2)
            attr_m = re.match(
                '.*ext="{(\w+?)}",null="{(\w+?)}",type="{(.+)}"', attributes
            )

            if node_name not in nodes:
                nodes[node_name] = CGFunction()
                nodes[node_name].label = node_name

            nodes[node_name].nodes += 1

            if attr_m:
                if "InternalCrate" in attr_m.group(3):
                    nodes[node_name].nodes_with_internal_crate += 1
                    nodes[node_name].type = attr_m.group(3)
                else:
                    nodes[node_name].nodes_without_internal_crate += 1

                if nodes[node_name].type == "":
                    nodes[node_name].type = attr_m.group(3)

                if attr_m.group(1) == "True":
                    nodes[node_name].nodes_external += 1
                if attr_m.group(2) == "False":
                    nodes[node_name].nodes_not_null += 1
                else:
                    nodes[node_name].nodes_null += 1

        else:
            g = re.match('\W*"(.*)" -> "(.*)";', line)
            if g:
                edges.add(g.group(0))
            else:
                print >> sys.stderr, "Could not match line '" + line + "'"

total = CGFunction()

nodes_with_internal_crate = 0
nodes_without_internal_crate = 0
nodes_merged_with_definition_expanded = 0

for node in nodes:
    node = nodes[node]
    total.nodes += node.nodes
    total.nodes_not_null += node.nodes_not_null
    total.nodes_null += node.nodes_null
    total.nodes_external += node.nodes_external
    if node.nodes_with_internal_crate > 0 and nodes_without_internal_crate > 0:
        nodes_merged_with_definition_expanded += 1
    elif node.nodes_with_internal_crate > 0:
        total.nodes_with_internal_crate += 1
        nodes_with_internal_crate += node.nodes_with_internal_crate
    elif node.nodes_without_internal_crate > 0:
        total.nodes_without_internal_crate += 1
        nodes_without_internal_crate += node.nodes_without_internal_crate
    print node

for edge in edges:
    print edge

print >> sys.stderr, "I reduced to " + str(len(nodes)) + " nodes, starting from " + str(
    total.nodes
) + "."
print >> sys.stderr, "Of these, " + str(
    total.nodes_not_null
) + " had a merged non-null node and " + str(total.nodes_null) + " did not."
print >> sys.stderr, str(
    total.nodes_with_internal_crate
) + " internalcrate-type merged nodes and " + str(
    total.nodes_without_internal_crate
) + " without."
print >> sys.stderr, "In total, " + str(
    nodes_with_internal_crate
) + " internalcrate-type nodes were merged and " + str(
    total.nodes_without_internal_crate
) + " without."
print >> sys.stderr, "nodes_merged_with_definition_expanded: " + str(
    nodes_merged_with_definition_expanded
)
print >> sys.stderr, "Moreover, " + str(
    total.nodes_external
) + " had an external merged node!"
