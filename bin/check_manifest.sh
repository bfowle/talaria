#!/bin/bash
# Quick diagnostic to check SwissProt chunk manifests

cd /home/brett/repos/talaria
./target/release/talaria database info uniprot/swissprot 2>&1 | head -50
