# coding=utf-8
#!/usr/bin/env python
# Reads-in a merged call graph and outputs a merged dependency graph
#
# (c) 2018 - onwards Moritz Beller <moritz beller @ gmx de>
#
# MIT/APACHE licensed -- check LICENSE files in top dir

import sys
import re
import os.path
import json

crates = set()
ufid_to_crate = {}
crate_dependencies = set()


def escape_label(label):
    return '"' + label + '"'


class Crate:
    def __init__(self):
        self.occurrences = 0
        self.source = []
        self.ufi = ""

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


class CrateDependency:
    def __init__(self):
        self.source
        self.target
        self.occurrences = 0

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
        m = re.match('\W*"(.*?)" \[(.*)\];', line)
        if m:
            node_name = m.group(1)

            deps = re.findall("(io :: crates :: (.+?) :: (.+?)) ", node_name)
            if deps is not None:
                deps = set(deps)
                if deps.__len__() > 0:
                    for item in deps:
                        source_crate = item[0]
                        crates.add(source_crate)

                    if deps.__len__() > 1:
                        # If there is more than one source crate, we try and find it by its internal (defining) crate
                        attributes = m.group(2)
                        namespaces = re.match(r".*type: \"(.*)\"", attributes)
                        namespace_list = json.loads(namespaces.group(1))
                        internal_crate = set(
                            [
                                ns["path"]
                                for ns in namespace_list
                                if ns["symbol"] == "InternalCrate"
                            ]
                        )

                        if len(internal_crate) == 1:
                            print >> sys.stderr, "Found multi source for '" + line + "'"
                            source_crate = internal_crate.pop()
                            ufid_to_crate[node_name] = source_crate
                            # Certain function symbols depend on structs and traits from other packages
                            external_crates = set(
                                [
                                    escape_label(source_crate)
                                    + " -> "
                                    + escape_label(ns["path"])
                                    for ns in namespace_list
                                    if ns["symbol"] == "ExternalCrate"
                                ]
                            )
                            for dep in external_crates:
                                if dep not in crate_dependencies:
                                    crate_dependencies.add(dep)
                        else:
                            # We could not find a source crate, hence we cannot embed this node
                            print >> sys.stderr, "Could not find source for '" + line + "'"
                            continue
                    else:
                        ufid_to_crate[node_name] = source_crate

        else:
            g = re.match('\W*"(.*)" -> "(.*)";', line)
            if g:
                if g.group(1) in ufid_to_crate and g.group(2) in ufid_to_crate:
                    source = ufid_to_crate[g.group(1)]
                    target = ufid_to_crate[g.group(2)]

                    if source == target:
                        # It is trivially true that a package depends on itself, skip such dependencies
                        continue

                    dep = escape_label(source) + " -> " + escape_label(target)
                    if dep not in crate_dependencies:
                        crate_dependencies.add(dep)
                else:
                    print >> sys.stderr, "Could not find both dependencies '" + g.group(
                        1
                    ) + "' and '" + g.group(2) + "'."

            else:
                print >> sys.stderr, "Could not match line '" + line + "'"


for node in crates:
    print node + ";"

for edge in crate_dependencies:
    print edge + ";"
