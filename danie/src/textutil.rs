pub fn strip_inline_markdown(text: &str) -> String {
    text.replace("**", "").replace("__", "").replace('`', "")
}

fn push_word(out: &mut Vec<String>, current: &mut String, word: &str, width: usize) {
    let mut rest = word;
    while rest.chars().count() > width {
        let cut = rest
            .char_indices()
            .nth(width)
            .map(|(i, _)| i)
            .unwrap_or(rest.len());
        out.push(rest[..cut].to_string());
        rest = &rest[cut..];
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
        } else if current.chars().count() + 1 + word.chars().count() <= width {
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
}
