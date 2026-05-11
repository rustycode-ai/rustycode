#!/bin/bash
set -e
head -1 data.csv > output.csv
tail -n +2 data.csv | sort -t',' -k2 -n >> output.csv
