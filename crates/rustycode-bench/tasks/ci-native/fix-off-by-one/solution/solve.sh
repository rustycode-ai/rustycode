#!/bin/bash
set -e
sed -i.bak 's/while left < right/while left <= right/' search.py
