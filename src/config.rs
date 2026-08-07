#[derive(Debug, Clone, PartialEq)]
struct DirectiveLine {
    keyword: String,
    value: String,
    raw: String,
    terminator: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Line {
    Directive(DirectiveLine),
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    lines: Vec<Line>,
}

fn parse_line(raw_line: &str) -> Line {
    let (content, terminator) = match raw_line.strip_suffix('\n') {
        Some(stripped) => (stripped, "\n"),
        None => (raw_line, ""),
    };

    let trimmed = content.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Line::Other(raw_line.to_string());
    }

    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let keyword = parts.next().unwrap_or("").to_string();
    let value = parts.next().unwrap_or("").trim().to_string();

    Line::Directive(DirectiveLine {
        keyword,
        value,
        raw: raw_line.to_string(),
        terminator: terminator.to_string(),
    })
}

impl Document {
    pub fn parse(input: &str) -> Self {
        let mut lines = Vec::new();
        let mut rest = input;
        while !rest.is_empty() {
            let line_end = rest.find('\n').map(|i| i + 1).unwrap_or(rest.len());
            let raw_line = &rest[..line_end];
            rest = &rest[line_end..];
            lines.push(parse_line(raw_line));
        }
        Document { lines }
    }

    pub fn get_directive(&self, keyword: &str) -> Option<&str> {
        self.lines.iter().find_map(|line| match line {
            Line::Directive(d) if d.keyword.eq_ignore_ascii_case(keyword) => Some(d.value.as_str()),
            _ => None,
        })
    }

    pub fn set_directive(&mut self, keyword: &str, value: &str) {
        for line in &mut self.lines {
            if let Line::Directive(d) = line {
                if !d.keyword.eq_ignore_ascii_case(keyword) {
                    continue;
                }
                d.value = value.to_string();
                d.raw = format!("{} {}{}", d.keyword, value, d.terminator);
                return;
            }
        }
        self.ensure_trailing_newline();
        self.lines.push(Line::Directive(DirectiveLine {
            keyword: keyword.to_string(),
            value: value.to_string(),
            raw: format!("{keyword} {value}\n"),
            terminator: "\n".to_string(),
        }));
    }

    fn ensure_trailing_newline(&mut self) {
        match self.lines.last_mut() {
            Some(Line::Directive(d)) if d.terminator.is_empty() => {
                d.raw.push('\n');
                d.terminator = "\n".to_string();
            }
            Some(Line::Other(raw)) if !raw.is_empty() && !raw.ends_with('\n') => {
                raw.push('\n');
            }
            _ => {}
        }
    }

    pub fn serialize(&self) -> String {
        self.lines
            .iter()
            .map(|line| match line {
                Line::Directive(d) => d.raw.as_str(),
                Line::Other(raw) => raw.as_str(),
            })
            .collect()
    }

    pub fn get_curated(&self, directive: CuratedDirective) -> Option<&str> {
        self.get_directive(directive.keyword())
    }

    pub fn set_curated(
        &mut self,
        directive: CuratedDirective,
        value: &str,
    ) -> Result<(), ValidationError> {
        validate(value)?;
        self.set_directive(directive.keyword(), value);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CuratedDirective {
    AudioDevice,
}

impl CuratedDirective {
    fn keyword(&self) -> &'static str {
        match self {
            CuratedDirective::AudioDevice => "ADEVICE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    Empty,
    ContainsNewline,
}

impl ValidationError {
    pub fn message(&self) -> &'static str {
        match self {
            ValidationError::Empty => "Value cannot be empty.",
            ValidationError::ContainsNewline => "Value cannot contain line breaks.",
        }
    }
}

// Deliberately generic rather than ADEVICE-specific: Direwolf accepts
// ADEVICE in several legitimately different shapes (an ALSA hardware name,
// a symbolic device name, "stdin"/"-", or a "capture playback" pair), and
// without an authoritative grammar for all of them, a stricter format check
// risks rejecting a value a real Direwolf install would accept.
fn validate(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return Err(ValidationError::Empty);
    }
    if value.contains('\n') || value.contains('\r') {
        return Err(ValidationError::ContainsNewline);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untouched_content_round_trips_byte_for_byte() {
        let input = "# a comment\nMYCALL N0CALL-1\n\nCHANNEL 0\n";

        let doc = Document::parse(input);

        assert_eq!(doc.serialize(), input);
    }

    #[test]
    fn round_trips_content_with_no_trailing_newline() {
        let input = "# a comment\nMYCALL N0CALL-1";

        let doc = Document::parse(input);

        assert_eq!(doc.serialize(), input);
    }

    #[test]
    fn get_directive_returns_the_value() {
        let doc = Document::parse("MYCALL N0CALL-1\n");

        assert_eq!(doc.get_directive("MYCALL"), Some("N0CALL-1"));
    }

    #[test]
    fn set_directive_changes_only_the_targeted_value_and_preserves_everything_else() {
        let input = "# rig notes: IC-7300 on /dev/ttyUSB0\nMYCALL N0CALL-1\n\nCHANNEL 0\nMODEM 1200\n";

        let mut doc = Document::parse(input);
        doc.set_directive("MYCALL", "W1AW-2");

        let expected = "# rig notes: IC-7300 on /dev/ttyUSB0\nMYCALL W1AW-2\n\nCHANNEL 0\nMODEM 1200\n";
        assert_eq!(doc.serialize(), expected);
    }

    #[test]
    fn set_directive_matches_keyword_case_insensitively() {
        let mut doc = Document::parse("mycall N0CALL-1\n");
        doc.set_directive("MYCALL", "W1AW-2");

        assert_eq!(doc.get_directive("mycall"), Some("W1AW-2"));
    }

    #[test]
    fn set_directive_appends_when_keyword_is_absent() {
        let mut doc = Document::parse("CHANNEL 0\n");
        doc.set_directive("MYCALL", "W1AW-2");

        assert_eq!(doc.serialize(), "CHANNEL 0\nMYCALL W1AW-2\n");
    }

    #[test]
    fn set_directive_appends_correctly_when_input_lacks_a_trailing_newline() {
        let mut doc = Document::parse("CHANNEL 0");
        doc.set_directive("MYCALL", "W1AW-2");

        assert_eq!(doc.serialize(), "CHANNEL 0\nMYCALL W1AW-2\n");
    }

    #[test]
    fn set_directive_appends_to_an_empty_document() {
        let mut doc = Document::parse("");
        doc.set_directive("MYCALL", "W1AW-2");

        assert_eq!(doc.serialize(), "MYCALL W1AW-2\n");
    }

    #[test]
    fn set_curated_updates_the_audio_device_and_preserves_everything_else() {
        let input = "# rig notes\nADEVICE plughw:0,0\n\nCHANNEL 0\n";
        let mut doc = Document::parse(input);

        doc.set_curated(CuratedDirective::AudioDevice, "plughw:1,0")
            .unwrap();

        assert_eq!(doc.get_curated(CuratedDirective::AudioDevice), Some("plughw:1,0"));
        assert_eq!(
            doc.serialize(),
            "# rig notes\nADEVICE plughw:1,0\n\nCHANNEL 0\n"
        );
    }

    #[test]
    fn set_curated_rejects_an_empty_value() {
        let mut doc = Document::parse("ADEVICE plughw:0,0\n");

        let result = doc.set_curated(CuratedDirective::AudioDevice, "   ");

        assert_eq!(result, Err(ValidationError::Empty));
    }

    #[test]
    fn set_curated_leaves_the_document_unchanged_on_validation_failure() {
        let input = "ADEVICE plughw:0,0\n";
        let mut doc = Document::parse(input);

        doc.set_curated(CuratedDirective::AudioDevice, "").ok();

        assert_eq!(doc.serialize(), input);
    }

    #[test]
    fn set_curated_rejects_a_value_containing_a_newline() {
        let mut doc = Document::parse("ADEVICE plughw:0,0\n");

        let result = doc.set_curated(CuratedDirective::AudioDevice, "plughw:1,0\nMYCALL W1AW-2");

        assert_eq!(result, Err(ValidationError::ContainsNewline));
    }
}
