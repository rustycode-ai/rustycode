#!/bin/bash
set -e
grep -oE 'https?://[^ ]+' input.txt | sort -f > urls.txt
