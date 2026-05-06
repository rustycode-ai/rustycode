// Shared markdown table helpers for the brutalist renderer.

/// Split a markdown table row into cells while respecting escaped pipes and
/// inline code spans.
pub(super) fn split_table_cells(row: &str) -> Vec<&str> {
    let row = row.trim().trim_matches('|');
    if row.is_empty() {
        return vec![""];
    }

    let mut cells = Vec::new();
    let mut cell_start = 0usize;
    let mut in_code_span = false;
    let mut code_fence_len = 0usize;
    let mut escape_next = false;
    let mut iter = row.char_indices().peekable();

    while let Some((idx, ch)) = iter.next() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match ch {
            '\\' => {
                escape_next = true;
            }
            '`' => {
                let mut fence_len = 1usize;
                while matches!(iter.peek(), Some((_, '`'))) {
                    iter.next();
                    fence_len += 1;
                }

                if in_code_span {
                    if fence_len == code_fence_len {
                        in_code_span = false;
                        code_fence_len = 0;
                    }
                } else {
                    in_code_span = true;
                    code_fence_len = fence_len;
                }
            }
            '|' if !in_code_span => {
                cells.push(row[cell_start..idx].trim());
                cell_start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    cells.push(row[cell_start..].trim());
    cells
}

/// Returns `true` if a table row looks like the markdown separator row.
pub(super) fn is_table_separator_row(row: &str) -> bool {
    let cells = split_table_cells(row);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let trimmed = cell.trim();
            !trimmed.is_empty()
                && trimmed
                    .chars()
                    .all(|c| c == '-' || c == ':' || c == ' ')
        })
}

#[cfg(test)]
mod table_tests {
    use super::{is_table_separator_row, split_table_cells};

    #[test]
    fn splits_cells_with_escaped_pipes_and_inline_code() {
        let cells = split_table_cells("| `x|y` | a \\| b |");
        assert_eq!(cells, vec!["`x|y`", "a \\| b"]);
    }

    #[test]
    fn recognizes_separator_rows() {
        assert!(is_table_separator_row("| --- | :---: |"));
        assert!(!is_table_separator_row("| name | value |"));
    }
}
