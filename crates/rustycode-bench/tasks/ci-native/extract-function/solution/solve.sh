#!/bin/bash
set -e
cat > calculate.py << 'EOF'
def calculate_circle_properties(radius):
    area = 3.14159 * radius * radius
    circumference = 2 * 3.14159 * radius
    return area, circumference

radius = float(input("Enter radius: "))
area, circumference = calculate_circle_properties(radius)
print(f"Area: {area}")
print(f"Circumference: {circumference}")
EOF
