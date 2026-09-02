#!/usr/bin/env bash
# Write one seed file per selector in the shared test corpus.
#
# The fuzzer starts from the selectors the test suite already covers, so
# it spends its budget on mutations of real syntax rather than on
# rediscovering that `div > p` parses.
set -euo pipefail

here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
corpus="$here/../tests/corpus/selectors.txt"
out="$here/corpus/translate"

mkdir -p "$out"
n=0
while IFS= read -r selector; do
  printf '%s' "$selector" > "$out/seed-$n"
  n=$((n + 1))
done < "$corpus"

echo "wrote $n seeds to $out"
