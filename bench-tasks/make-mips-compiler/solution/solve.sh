#!/bin/bash
set -e

# Oracle solution: copy the reference assembler to /app and make it executable
cp /solution/mips_asm.py /app/mips_asm.py
chmod +x /app/mips_asm.py
