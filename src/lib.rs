use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    Markdown,
    Xml,
}

impl Format {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Xml => "xml",
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire())
    }
}

impl FromStr for Format {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "md" | "markdown" => Ok(Self::Markdown),
            "xml" | "paste" => Ok(Self::Xml),
            _ => Err(format!("unknown format '{value}', expected md or xml")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WrappedFormat {
    Markdown,
    Xml,
}

pub fn wrap_content(content: &str, format: Format) -> String {
    match format {
        Format::Markdown => wrap_markdown(content),
        Format::Xml => wrap_xml(content),
    }
}

pub fn transform_clipboard_for_paste(content: &str, format: Format) -> String {
    match (detect_wrapped_format(content), format) {
        (Some(WrappedFormat::Markdown), Format::Markdown)
        | (Some(WrappedFormat::Xml), Format::Xml) => trim_trailing_newlines(content).to_string(),
        (Some(WrappedFormat::Markdown), Format::Xml) => wrap_xml(&unwrap_markdown(content)),
        (Some(WrappedFormat::Xml), Format::Markdown) => wrap_markdown(&unwrap_xml(content)),
        (None, format) => wrap_content(content, format),
    }
}

pub fn unwrap_auto(content: &str) -> String {
    match detect_wrapped_format(content) {
        Some(WrappedFormat::Markdown) => unwrap_markdown(content),
        Some(WrappedFormat::Xml) => unwrap_xml(content),
        None => trim_trailing_newlines(content).to_string(),
    }
}

pub fn detect_wrapped_format(content: &str) -> Option<WrappedFormat> {
    if is_wrapped_markdown(content) {
        Some(WrappedFormat::Markdown)
    } else if is_wrapped_xml(content) {
        Some(WrappedFormat::Xml)
    } else {
        None
    }
}

pub fn longest_backtick_run(content: &str) -> usize {
    let mut max = 0;
    let mut current = 0;

    for ch in content.chars() {
        if ch == '`' {
            current += 1;
        } else {
            max = max.max(current);
            current = 0;
        }
    }

    max.max(current)
}

fn wrap_markdown(content: &str) -> String {
    let content = trim_trailing_newlines(content);
    let longest = longest_backtick_run(content);
    let fence_len = if longest >= 3 { longest + 2 } else { 3 };
    let fence = "`".repeat(fence_len);

    format!("{fence}\n{content}\n{fence}\n")
}

fn wrap_xml(content: &str) -> String {
    let content = trim_trailing_newlines(content);

    format!("<paste>\n{content}\n</paste>\n")
}

fn unwrap_markdown(content: &str) -> String {
    let lines = trim_empty_lines(content.lines().collect::<Vec<_>>());
    if lines.len() < 2 {
        return content.to_string();
    }

    let longest = longest_backtick_run(content);
    if longest < 3 {
        return content.to_string();
    }

    let fence = "`".repeat(longest);
    if lines.first().map(|line| line.trim()) != Some(fence.as_str())
        || lines.last().map(|line| line.trim()) != Some(fence.as_str())
    {
        return content.to_string();
    }

    lines[1..lines.len() - 1].join("\n")
}

fn unwrap_xml(content: &str) -> String {
    let lines = trim_empty_lines(content.lines().collect::<Vec<_>>());
    if lines.len() < 2 {
        return content.to_string();
    }

    if lines.first().map(|line| line.trim()) != Some("<paste>")
        || lines.last().map(|line| line.trim()) != Some("</paste>")
    {
        return content.to_string();
    }

    let middle = lines[1..lines.len() - 1].join("\n");
    if is_wrapped_markdown(&middle) {
        trim_trailing_newlines(&middle).to_string()
    } else {
        middle
    }
}

fn is_wrapped_xml(content: &str) -> bool {
    let lines = trim_empty_lines(content.lines().collect::<Vec<_>>());

    lines.first().map(|line| line.trim()) == Some("<paste>")
        && lines.last().map(|line| line.trim()) == Some("</paste>")
}

fn is_wrapped_markdown(content: &str) -> bool {
    let lines = trim_empty_lines(content.lines().collect::<Vec<_>>());
    if lines.len() < 2 {
        return false;
    }

    let longest = longest_backtick_run(content);
    if longest < 3 {
        return false;
    }

    let fence = "`".repeat(longest);

    lines.first().map(|line| line.trim()) == Some(fence.as_str())
        && lines.last().map(|line| line.trim()) == Some(fence.as_str())
}

fn trim_trailing_newlines(content: &str) -> &str {
    content.trim_end_matches('\n')
}

fn trim_empty_lines(mut lines: Vec<&str>) -> Vec<&str> {
    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }

    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_wraps_with_three_backticks_by_default() {
        assert_eq!(wrap_content("hello", Format::Markdown), "```\nhello\n```\n");
    }

    #[test]
    fn markdown_wraps_with_adaptive_fence() {
        assert_eq!(
            wrap_content("a\n```nested\nb", Format::Markdown),
            "`````\na\n```nested\nb\n`````\n"
        );
    }

    #[test]
    fn xml_wraps_with_paste_tags() {
        assert_eq!(
            wrap_content("hello\n", Format::Xml),
            "<paste>\nhello\n</paste>\n"
        );
    }

    #[test]
    fn detects_plain_fences_but_not_labeled_fences() {
        assert_eq!(
            detect_wrapped_format("```\nhello\n```"),
            Some(WrappedFormat::Markdown)
        );
        assert_eq!(detect_wrapped_format("```rust\nhello\n```"), None);
    }

    #[test]
    fn paste_transform_is_idempotent_for_same_format() {
        assert_eq!(
            transform_clipboard_for_paste("```\nhello\n```\n", Format::Markdown),
            "```\nhello\n```"
        );
    }

    #[test]
    fn paste_transform_converts_between_formats() {
        assert_eq!(
            transform_clipboard_for_paste("```\nhello\n```\n", Format::Xml),
            "<paste>\nhello\n</paste>\n"
        );
        assert_eq!(
            transform_clipboard_for_paste("<paste>\nhello\n</paste>\n", Format::Markdown),
            "```\nhello\n```\n"
        );
    }

    #[test]
    fn unwrap_auto_handles_markdown_and_xml() {
        assert_eq!(unwrap_auto("```\nhello\n```"), "hello");
        assert_eq!(unwrap_auto("<paste>\nhello\n</paste>"), "hello");
    }

    #[test]
    fn unwrap_xml_keeps_inner_labeled_fence() {
        assert_eq!(
            unwrap_auto("<paste>\n```rust\nhello\n```\n</paste>"),
            "```rust\nhello\n```"
        );
    }
}
