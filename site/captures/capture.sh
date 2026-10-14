#!/usr/bin/env bash
# Regenerates the terminal captures shown on the site: the help output of the release
# binary. Help needs no Google login, so every capture is real and reproducible.
#   site/captures/capture.sh   (after cargo build --release)
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
out="$repo/site/captures"
bin="$repo/target/release/auditmyvisitors"

"$bin" --version > "$out/version.txt"
"$bin" --help > "$out/help.txt"
"$bin" report --help > "$out/report.txt"
"$bin" report top-pages --help > "$out/report-top-pages.txt"
"$bin" report queries --help > "$out/report-queries.txt"
"$bin" report compare --help > "$out/report-compare.txt"
"$bin" export pdf --help > "$out/export-pdf.txt"
"$bin" export csv --help > "$out/export-csv.txt"
