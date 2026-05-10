#!/bin/bash
set -e

# Create test files
echo "notes content" > notes.txt
echo "report content" > report.txt
echo "data content" > data.json

# Check if backup script exists
if [ ! -f "backup.sh" ]; then
    echo "FAIL: backup.sh not found"
    exit 1
fi

chmod +x backup.sh

# Run the backup script
./backup.sh "*.txt"

# Check if backups directory was created
if [ ! -d "backups" ]; then
    echo "FAIL: backups/ directory not created"
    exit 1
fi

# Check if txt files were backed up
backup_count=$(find backups -name "*.txt" | wc -l)
if [ "$backup_count" -lt 2 ]; then
    echo "FAIL: Expected at least 2 .txt backups, found $backup_count"
    exit 1
fi

# Check if json file was NOT backed up
if find backups -name "*.json" | grep -q .; then
    echo "FAIL: .json files should not be backed up when pattern is *.txt"
    exit 1
fi

# Check timestamp format (should be YYYYMMDD_HHMMSS_filename)
backup_file=$(find backups -name "*.txt" | head -1)
filename=$(basename "$backup_file")
if [[ ! "$filename" =~ ^[0-9]{8}_[0-9]{6}_ ]]; then
    echo "FAIL: Backup file doesn't have correct timestamp format: $filename"
    exit 1
fi

# Check if original files still exist
if [ ! -f "notes.txt" ]; then
    echo "FAIL: Original notes.txt should still exist"
    exit 1
fi

# Check backup content
backup_content=$(find backups -name "*_notes.txt" -exec cat {} \;)
if [ "$backup_content" != "notes content" ]; then
    echo "FAIL: Backup content doesn't match original"
    exit 1
fi

echo "PASS: Backup script works correctly"
exit 0
