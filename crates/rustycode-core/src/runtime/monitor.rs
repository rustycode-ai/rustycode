//! Agent runtime monitoring and repetition detection.
//! Shared logic for all UI modes (Headless, TUI, CLI).

pub struct AgentMonitor {
    pub repetition_check_threshold: usize,
}

impl AgentMonitor {
    pub fn new(threshold: usize) -> Self {
        Self {
            repetition_check_threshold: threshold,
        }
    }
}

pub fn detect_and_truncate_repeated_blocks(text: &str) -> Option<String> {
    if text.len() < 200 {
        return None;
    }

    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    if paragraphs.len() < 6 {
        return None;
    }

    for block_size in 3..=8.min(paragraphs.len() / 2) {
        let first_block: Vec<&str> = paragraphs[..block_size].to_vec();
        let first_block_text = first_block.join("\n\n");

        if first_block_text.len() < 100 {
            continue;
        }

        let mut repetitions = 1;
        let mut pos = block_size;

        while pos + block_size <= paragraphs.len() {
            let candidate: Vec<&str> = paragraphs[pos..pos + block_size].to_vec();
            let candidate_text = candidate.join("\n\n");

            if blocks_match(&first_block_text, &candidate_text) {
                repetitions += 1;
                pos += block_size;
            } else {
                break;
            }
        }

        if repetitions >= 3 {
            let end_of_repetitions = block_size * repetitions;
            let mut result_parts: Vec<&str> = paragraphs[..block_size].to_vec();

            if end_of_repetitions < paragraphs.len() {
                result_parts.extend_from_slice(&paragraphs[end_of_repetitions..]);
            }

            return Some(result_parts.join("\n\n"));
        }
    }

    None
}

fn blocks_match(a: &str, b: &str) -> bool {
    let a_normalized: String = a.chars().filter(|c| !c.is_whitespace()).collect();
    let b_normalized: String = b.chars().filter(|c| !c.is_whitespace()).collect();
    if a_normalized.len() < 50 {
        return false;
    }
    if a_normalized == b_normalized {
        return true;
    }
    let min_len = a_normalized.len().min(b_normalized.len());
    if min_len < 50 {
        return false;
    }
    let check_len = (min_len as f64 * 0.8) as usize;
    if check_len < 50 {
        return false;
    }
    let matching: usize = a_normalized[..check_len]
        .chars()
        .zip(b_normalized[..check_len].chars())
        .map(|(a, b)| if a == b { 1 } else { 0 })
        .sum();
    (matching as f64 / check_len as f64) > 0.9
}

pub fn strip_repeated_preamble_phrases(text: &str) -> String {
    let sentences: Vec<&str> = text
        .split_inclusive(['.', '!', '?'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if sentences.len() < 3 {
        return text.to_string();
    }

    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for s in &sentences {
        *counts.entry(s).or_insert(0) += 1;
    }

    let repeated: std::collections::HashSet<&&str> = counts
        .iter()
        .filter(|(_, &c)| c >= 3)
        .map(|(s, _)| s)
        .collect();

    if repeated.is_empty() {
        return text.to_string();
    }

    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut result = String::new();
    for s in &sentences {
        if repeated.contains(&s) {
            if !seen.contains(s) {
                seen.insert(s);
                result.push_str(s);
                result.push(' ');
            }
        } else {
            result.push_str(s);
        }
    }

    result.trim().to_string()
}

pub fn strip_repeated_prefix(current: &str, previous: &str) -> String {
    if current.is_empty() || previous.is_empty() {
        return current.to_string();
    }

    let current_lines: Vec<&str> = current.lines().collect();
    let previous_lines: Vec<&str> = previous.lines().collect();

    let mut match_count = 0;
    for (cur_line, prev_line) in current_lines.iter().zip(previous_lines.iter()) {
        if cur_line.trim() == prev_line.trim() && !cur_line.trim().is_empty() {
            match_count += 1;
        } else {
            break;
        }
    }

    if match_count >= 3 {
        let remaining = &current_lines[match_count..];
        remaining.join("\n").trim().to_string()
    } else {
        current.to_string()
    }
}

pub fn detect_tool_loop(recent: &[String], min_length: usize) -> Option<String> {
    if recent.len() < min_length {
        return None;
    }
    for period in 1..=3 {
        let check_len = std::cmp::max(min_length, 2 * period);
        if recent.len() < check_len {
            continue;
        }
        let tail = &recent[recent.len() - check_len..];
        let is_repeating = tail
            .iter()
            .enumerate()
            .all(|(i, name)| *name == tail[i % period]);
        if is_repeating {
            let pattern: Vec<&str> = tail[..period].iter().map(|s| s.as_str()).collect();
            let repetitions = check_len / period;
            return Some(format!(
                "[{}] repeated {} times",
                pattern.join(" -> "),
                repetitions
            ));
        }
    }
    None
}

pub fn text_indicates_completion(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("task completed")
        || lower.contains("task is complete")
        || lower.contains("task is done")
        || lower.contains("task is finished")
        || lower.contains("all tests pass")
        || lower.contains("all tests passed")
        || lower.contains("all tests passing")
        || lower.contains("successfully completed")
        || lower.contains("solution is complete")
        || lower.contains("solution works correctly")
        || lower.contains("solution is working")
        || lower.contains("problem is solved")
        || lower.contains("implementation is complete")
        || lower.contains("changes are verified")
        || lower.contains("done. the task is")
        || lower.contains("the task has been completed")
        || lower.contains("mission accomplished")
        || lower.contains("solution verified")
        || lower.contains("output matches expected")
        || lower.contains("produces the correct output")
        || lower.contains("all assertions pass")
        || lower.contains("verification successful")
        || lower.contains("verified successfully")
        || lower.contains("everything works")
        || lower.contains("working correctly")
        || lower.contains("test results: all passed")
        || lower.contains("0 failures")
        || lower.contains("0 failed")
        || lower.contains("no errors")
        || lower.contains("passing all tests")
}

pub fn text_indicates_giving_up(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("i cannot complete")
        || lower.contains("i'm unable to")
        || lower.contains("i am unable to")
        || lower.contains("cannot be solved")
        || lower.contains("not possible to")
        || lower.contains("unable to proceed")
        || lower.contains("i've tried everything")
        || lower.contains("no further action")
        || lower.contains("nothing more i can")
        || lower.contains("i don't think this can")
        || lower.contains("beyond my capabilities")
        || lower.contains("does not seem to be")
        || lower.contains("there's nothing else")
        || lower.contains("i've already tried all")
        || lower.contains("i give up")
        || lower.contains("exhausted all options")
        || lower.contains("i'm not sure how to")
        || lower.contains("this doesn't seem to work")
}
