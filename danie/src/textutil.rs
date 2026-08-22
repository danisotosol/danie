use unicode_segmentation::UnicodeSegmentation;

pub fn strip_inline_markdown(text: &str) -> String {
    text.replace("**", "").replace("__", "").replace('`', "")
}

fn grapheme_len(text: &str) -> usize {
    text.graphemes(true).count()
}

/// Splits `word` after `width` grapheme clusters.
fn split_at_graphemes(word: &str, width: usize) -> (&str, &str) {
    let cut = word
        .grapheme_indices(true)
        .nth(width)
        .map(|(i, _)| i)
        .unwrap_or(word.len());
    word.split_at(cut)
}

fn push_word(out: &mut Vec<String>, current: &mut String, word: &str, width: usize) {
    let mut rest = word;
    while grapheme_len(rest) > width {
        let (head, tail) = split_at_graphemes(rest, width);
        if !head.is_empty() {
            out.push(head.to_string());
        }
        rest = tail;
    }
    if !rest.is_empty() {
        current.push_str(rest);
    }
}

pub fn wrap_line(line: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in line.split_whitespace() {
        if current.is_empty() {
            push_word(&mut lines, &mut current, word, width);
        } else if grapheme_len(&current) + 1 + grapheme_len(word) <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            push_word(&mut lines, &mut current, word, width);
        }
    }
    lines.push(current);
    lines
}

pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    text.split('\n')
        .flat_map(|line| wrap_line(line, width))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_long_words_harder_than_width() {
        assert_eq!(wrap_line("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn wraps_on_word_boundaries_and_keeps_blank_lines() {
        assert_eq!(wrap_line("hello brave new world", 11), vec![
            "hello brave", "new world"
        ]);
        assert_eq!(wrap_text("a\n\nb", 10), vec!["a", "", "b"]);
    }

    #[test]
    fn strips_bold_and_code_markers() {
        assert_eq!(strip_inline_markdown("**bold** and `code`"), "bold and code");
    }

    #[test]
    fn never_splits_grapheme_clusters() {
        let combined = "e\u{301}";
        let word = combined.repeat(4);
        assert_eq!(wrap_line(&word, 2), vec![combined.repeat(2), combined.repeat(2)]);

        let flag = "\u{1F1EA}\u{1F1F8}";
        assert_eq!(wrap_line(&flag.repeat(3), 2), vec![flag.repeat(2), flag.to_string()]);
    }

    #[test]
    fn ascii_behavior_is_unchanged() {
        let long = "x".repeat(7);
        assert_eq!(wrap_line(&long, 3), vec!["xxx", "xxx", "x"]);
    }
}
