#!/bin/bash
set -e
python3 -c "
import re
from collections import Counter

with open('input.txt') as f:
    text = f.read().lower()
words = re.findall(r'[a-z]+', text)
counts = Counter(words)
sorted_words = sorted(counts.items(), key=lambda x: (-x[1], x[0]))[:10]
with open('wordcount.txt', 'w') as f:
    for word, count in sorted_words:
        f.write(f'{word}: {count}\n')
"
