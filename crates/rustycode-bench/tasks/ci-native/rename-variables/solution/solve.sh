#!/bin/bash
set -e
cat > process.py << 'EOF'
def calc(quantity, unit_price, tax_base):
    subtotal = quantity * unit_price
    total = subtotal + tax_base
    tax = total * 0.1
    return tax

result = calc(10, 5, 3)
print(result)
EOF
