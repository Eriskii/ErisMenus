#!/bin/zsh
# usage: dis.sh name...  (disassembles in a stopped ./board at main)
f=$(mktemp); { echo "settings set auto-confirm true"; echo "br set -s board -n main"; echo "run"; for n in "$@"; do echo "disassemble -n \"$n\""; done; echo quit; } > $f
lldb -b -s $f -- /private/tmp/claude-501/-Users-eris-Documents-Programming-Projects/c62d994a-2835-49f1-8487-c1f881b93af1/scratchpad/accent/w/board /dev/null 2>&1 | sed -n '/^(lldb) disassemble/,$p' | grep -v '^Process'
