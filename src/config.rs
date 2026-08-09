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

    // NOTE: get_curated/set_curated always target the *first* occurrence of
    // a directive's keyword in the file (via get_directive/set_directive).
    // For CHANNEL specifically this is a real simplification: Direwolf's
    // CHANNEL directive is a repeating selector — a multi-channel config
    // has multiple "CHANNEL n" blocks, each with its own MODEM/PTT lines
    // scoped beneath it. This curated model edits only the first channel's
    // settings; a config with channel 1+ has those settings invisible and
    // unreachable through the curated form (though still safely preserved
    // as Raw Directives and editable via the raw-text view). Extending to
    // multiple channels would need a different, channel-aware model, not
    // just more CuratedDirective variants.
    pub fn get_curated(&self, directive: CuratedDirective) -> Option<&str> {
        self.get_directive(directive.spec().keyword)
    }

    pub fn set_curated(
        &mut self,
        directive: CuratedDirective,
        value: &str,
    ) -> Result<(), ValidationError> {
        let spec = directive.spec();

        // A blank value for a directive with no meaningful existing value
        // — absent entirely, or present as a bare line with no arguments
        // — is not an error, just means the user isn't using that
        // directive. Only a blank value for a directive that already has
        // a real value is rejected (see set_curated_rejects_an_empty_value):
        // clearing an existing directive's value through this form isn't
        // supported.
        let has_existing_value = self
            .get_directive(spec.keyword)
            .is_some_and(|v| !v.trim().is_empty());
        if value.trim().is_empty() && !has_existing_value {
            return Ok(());
        }

        (spec.validate)(value)?;
        self.set_directive(spec.keyword, value);
        Ok(())
    }
}

struct DirectiveSpec {
    keyword: &'static str,
    validate: fn(&str) -> Result<(), ValidationError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CuratedDirective {
    AudioDevice,
    Channel,
    Modem,
    Ptt,
    AgwPort,
    KissPort,
    PBeacon,
    CBeacon,
    Digipeat,
}

impl CuratedDirective {
    // Each variant's full behavior (keyword + validator) lives here, in one
    // place — deliberately not split across a separate keyword() match and
    // a separate validate() match, so adding a variant never requires
    // keeping two switches in sync.
    fn spec(&self) -> DirectiveSpec {
        match self {
            CuratedDirective::AudioDevice => DirectiveSpec {
                keyword: "ADEVICE",
                validate: validate_generic,
            },
            CuratedDirective::Channel => DirectiveSpec {
                keyword: "CHANNEL",
                validate: validate_non_negative_integer,
            },
            CuratedDirective::Modem => DirectiveSpec {
                keyword: "MODEM",
                validate: validate_numeric_prefix,
            },
            CuratedDirective::Ptt => DirectiveSpec {
                keyword: "PTT",
                validate: validate_generic,
            },
            CuratedDirective::AgwPort => DirectiveSpec {
                keyword: "AGWPORT",
                validate: validate_port,
            },
            CuratedDirective::KissPort => DirectiveSpec {
                keyword: "KISSPORT",
                validate: validate_port,
            },
            CuratedDirective::PBeacon => DirectiveSpec {
                keyword: "PBEACON",
                validate: validate_generic,
            },
            CuratedDirective::CBeacon => DirectiveSpec {
                keyword: "CBEACON",
                validate: validate_generic,
            },
            // DIGIPEAT's leading token (from-chan) is always a channel
            // number; the remaining to-chan/aliases/wide/preemptive tokens
            // share validate_numeric_prefix's rationale for not going
            // further — same leading-numeric-token shape as MODEM's baud
            // rate, no authoritative grammar to check the rest against.
            CuratedDirective::Digipeat => DirectiveSpec {
                keyword: "DIGIPEAT",
                validate: validate_numeric_prefix,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    Empty,
    ContainsNewline,
    NotANumber,
    MissingNumericPrefix,
    OutOfRange,
}

impl ValidationError {
    pub fn message(&self) -> &'static str {
        match self {
            ValidationError::Empty => "Value cannot be empty.",
            ValidationError::ContainsNewline => "Value cannot contain line breaks.",
            ValidationError::NotANumber => "Value must be a whole number.",
            ValidationError::MissingNumericPrefix => "Value must start with a number.",
            ValidationError::OutOfRange => "Value must be a port number from 0 to 65535.",
        }
    }
}

fn validate_generic(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return Err(ValidationError::Empty);
    }
    if value.contains('\n') || value.contains('\r') {
        return Err(ValidationError::ContainsNewline);
    }
    Ok(())
}

// CHANNEL selects which channel subsequent directives apply to; Direwolf
// numbers channels as non-negative integers starting at 0.
fn validate_non_negative_integer(value: &str) -> Result<(), ValidationError> {
    validate_generic(value)?;
    if value.trim().parse::<u32>().is_err() {
        return Err(ValidationError::NotANumber);
    }
    Ok(())
}

// Shared by directives whose first token is always a confirmed-numeric
// value — MODEM's baud rate, DIGIPEAT's from-chan. Direwolf allows
// additional trailing parameters (MODEM's waveform type, DIGIPEAT's
// to-chan/aliases/wide/preemptive) whose grammar isn't validated here for
// the same reason ADEVICE's value isn't validated beyond generic checks —
// no authoritative grammar to check them against.
fn validate_numeric_prefix(value: &str) -> Result<(), ValidationError> {
    validate_generic(value)?;
    let first_token = value.split_whitespace().next().unwrap_or("");
    if first_token.parse::<u32>().is_err() {
        return Err(ValidationError::MissingNumericPrefix);
    }
    Ok(())
}

// AGWPORT/KISSPORT are TCP port numbers, so the full unsigned 16-bit range
// (0-65535) applies. 0 is deliberately accepted rather than rejected as
// out-of-range: Direwolf's convention (as with other server-style config
// tools) is that a port of 0 disables that particular network interface.
// This isn't confirmed against Direwolf's own source or docs, same caveat
// as MODEM's trailing-parameter handling above — but rejecting it outright
// risks blocking a legitimate way to turn a port off, which seems like the
// worse failure mode if this assumption turns out wrong.
fn validate_port(value: &str) -> Result<(), ValidationError> {
    validate_generic(value)?;
    match value.trim().parse::<u32>() {
        Ok(n) if n <= 65535 => Ok(()),
        Ok(_) => Err(ValidationError::OutOfRange),
        Err(_) => Err(ValidationError::NotANumber),
    }
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
    fn set_curated_is_a_no_op_when_blank_and_the_directive_was_never_set() {
        let input = "# rig notes\nCHANNEL 0\n";
        let mut doc = Document::parse(input);

        let result = doc.set_curated(CuratedDirective::CBeacon, "");

        assert_eq!(result, Ok(()));
        assert_eq!(doc.get_curated(CuratedDirective::CBeacon), None);
        assert_eq!(doc.serialize(), input);
    }

    #[test]
    fn set_curated_is_a_no_op_when_whitespace_only_and_the_directive_was_never_set() {
        let input = "CHANNEL 0\n";
        let mut doc = Document::parse(input);

        let result = doc.set_curated(CuratedDirective::PBeacon, "   ");

        assert_eq!(result, Ok(()));
        assert_eq!(doc.get_curated(CuratedDirective::PBeacon), None);
        assert_eq!(doc.serialize(), input);
    }

    #[test]
    fn set_curated_is_a_no_op_when_blank_and_the_directive_line_is_present_but_valueless() {
        // A bare directive line with no arguments (e.g. hand-written, or
        // left over from another tool) has no meaningful value even
        // though get_directive finds the line — treat it the same as
        // "never set" rather than rejecting it as a cleared field.
        let input = "CHANNEL 0\nCBEACON\n";
        let mut doc = Document::parse(input);

        let result = doc.set_curated(CuratedDirective::CBeacon, "");

        assert_eq!(result, Ok(()));
        assert_eq!(doc.serialize(), input);
    }

    #[test]
    fn set_curated_no_op_exemption_applies_to_numerically_validated_directives_too() {
        let input = "ADEVICE plughw:0,0\n";
        let mut doc = Document::parse(input);

        let result = doc.set_curated(CuratedDirective::Digipeat, "");

        assert_eq!(result, Ok(()));
        assert_eq!(doc.get_curated(CuratedDirective::Digipeat), None);
        assert_eq!(doc.serialize(), input);
    }

    #[test]
    fn set_curated_rejects_a_value_containing_a_newline() {
        let mut doc = Document::parse("ADEVICE plughw:0,0\n");

        let result = doc.set_curated(CuratedDirective::AudioDevice, "plughw:1,0\nMYCALL W1AW-2");

        assert_eq!(result, Err(ValidationError::ContainsNewline));
    }

    #[test]
    fn set_curated_updates_the_channel_and_preserves_everything_else() {
        let input = "# rig notes\nCHANNEL 0\n\nADEVICE plughw:0,0\n";
        let mut doc = Document::parse(input);

        doc.set_curated(CuratedDirective::Channel, "1").unwrap();

        assert_eq!(doc.get_curated(CuratedDirective::Channel), Some("1"));
        assert_eq!(doc.serialize(), "# rig notes\nCHANNEL 1\n\nADEVICE plughw:0,0\n");
    }

    #[test]
    fn set_curated_rejects_a_non_numeric_channel() {
        let mut doc = Document::parse("CHANNEL 0\n");

        let result = doc.set_curated(CuratedDirective::Channel, "abc");

        assert_eq!(result, Err(ValidationError::NotANumber));
    }

    #[test]
    fn set_curated_rejects_a_negative_channel() {
        let mut doc = Document::parse("CHANNEL 0\n");

        let result = doc.set_curated(CuratedDirective::Channel, "-1");

        assert_eq!(result, Err(ValidationError::NotANumber));
    }

    #[test]
    fn set_curated_updates_the_modem_and_preserves_everything_else() {
        let input = "# rig notes\nMODEM 1200\n\nADEVICE plughw:0,0\n";
        let mut doc = Document::parse(input);

        doc.set_curated(CuratedDirective::Modem, "9600").unwrap();

        assert_eq!(doc.get_curated(CuratedDirective::Modem), Some("9600"));
        assert_eq!(doc.serialize(), "# rig notes\nMODEM 9600\n\nADEVICE plughw:0,0\n");
    }

    #[test]
    fn set_curated_accepts_modem_trailing_parameters_after_the_baud_rate() {
        let mut doc = Document::parse("MODEM 1200\n");

        let result = doc.set_curated(CuratedDirective::Modem, "9600 D");

        assert!(result.is_ok());
        assert_eq!(doc.get_curated(CuratedDirective::Modem), Some("9600 D"));
    }

    #[test]
    fn set_curated_rejects_a_modem_value_without_a_leading_baud_rate() {
        let mut doc = Document::parse("MODEM 1200\n");

        let result = doc.set_curated(CuratedDirective::Modem, "fast");

        assert_eq!(result, Err(ValidationError::MissingNumericPrefix));
    }

    #[test]
    fn set_curated_updates_ptt_and_preserves_everything_else() {
        let input = "# rig notes\nPTT COM1 RTS\n\nADEVICE plughw:0,0\n";
        let mut doc = Document::parse(input);

        doc.set_curated(CuratedDirective::Ptt, "GPIO 25").unwrap();

        assert_eq!(doc.get_curated(CuratedDirective::Ptt), Some("GPIO 25"));
        assert_eq!(doc.serialize(), "# rig notes\nPTT GPIO 25\n\nADEVICE plughw:0,0\n");
    }

    #[test]
    fn set_curated_rejects_an_empty_ptt_value() {
        let mut doc = Document::parse("PTT COM1 RTS\n");

        let result = doc.set_curated(CuratedDirective::Ptt, "");

        assert_eq!(result, Err(ValidationError::Empty));
    }

    #[test]
    fn set_curated_updates_agwport_and_preserves_everything_else() {
        let input = "# rig notes\nAGWPORT 8000\n\nADEVICE plughw:0,0\n";
        let mut doc = Document::parse(input);

        doc.set_curated(CuratedDirective::AgwPort, "8010").unwrap();

        assert_eq!(doc.get_curated(CuratedDirective::AgwPort), Some("8010"));
        assert_eq!(doc.serialize(), "# rig notes\nAGWPORT 8010\n\nADEVICE plughw:0,0\n");
    }

    #[test]
    fn set_curated_accepts_zero_as_a_port_value_meaning_disabled() {
        let mut doc = Document::parse("AGWPORT 8000\n");

        let result = doc.set_curated(CuratedDirective::AgwPort, "0");

        assert!(result.is_ok());
        assert_eq!(doc.get_curated(CuratedDirective::AgwPort), Some("0"));
    }

    #[test]
    fn set_curated_rejects_a_port_above_65535() {
        let mut doc = Document::parse("AGWPORT 8000\n");

        let result = doc.set_curated(CuratedDirective::AgwPort, "99999");

        assert_eq!(result, Err(ValidationError::OutOfRange));
    }

    #[test]
    fn set_curated_rejects_a_non_numeric_port() {
        let mut doc = Document::parse("AGWPORT 8000\n");

        let result = doc.set_curated(CuratedDirective::AgwPort, "eight-thousand");

        assert_eq!(result, Err(ValidationError::NotANumber));
    }

    #[test]
    fn set_curated_updates_kissport_and_preserves_everything_else() {
        let input = "# rig notes\nKISSPORT 8001\n\nADEVICE plughw:0,0\n";
        let mut doc = Document::parse(input);

        doc.set_curated(CuratedDirective::KissPort, "8011").unwrap();

        assert_eq!(doc.get_curated(CuratedDirective::KissPort), Some("8011"));
        assert_eq!(doc.serialize(), "# rig notes\nKISSPORT 8011\n\nADEVICE plughw:0,0\n");
    }

    #[test]
    fn set_curated_updates_pbeacon_and_preserves_everything_else() {
        let input = "# rig notes\nPBEACON delay=1 every=30 lat=42^37.14N long=071^20.83W\n\nADEVICE plughw:0,0\n";
        let mut doc = Document::parse(input);

        doc.set_curated(CuratedDirective::PBeacon, "delay=1 every=10 lat=42^37.14N long=071^20.83W")
            .unwrap();

        assert_eq!(
            doc.get_curated(CuratedDirective::PBeacon),
            Some("delay=1 every=10 lat=42^37.14N long=071^20.83W")
        );
        assert_eq!(
            doc.serialize(),
            "# rig notes\nPBEACON delay=1 every=10 lat=42^37.14N long=071^20.83W\n\nADEVICE plughw:0,0\n"
        );
    }

    #[test]
    fn set_curated_updates_cbeacon_and_preserves_everything_else() {
        let input = "# rig notes\nCBEACON delay=1 info=\"Test\"\n\nADEVICE plughw:0,0\n";
        let mut doc = Document::parse(input);

        doc.set_curated(CuratedDirective::CBeacon, "delay=1 info=\"Updated\"")
            .unwrap();

        assert_eq!(
            doc.get_curated(CuratedDirective::CBeacon),
            Some("delay=1 info=\"Updated\"")
        );
        assert_eq!(
            doc.serialize(),
            "# rig notes\nCBEACON delay=1 info=\"Updated\"\n\nADEVICE plughw:0,0\n"
        );
    }

    #[test]
    fn set_curated_updates_digipeat_and_preserves_everything_else() {
        let input = "# rig notes\nDIGIPEAT 0 0 WIDE1-1,WIDE2-1 TRACE\n\nADEVICE plughw:0,0\n";
        let mut doc = Document::parse(input);

        doc.set_curated(CuratedDirective::Digipeat, "0 1 WIDE1-1,WIDE2-1 TRACE")
            .unwrap();

        assert_eq!(
            doc.get_curated(CuratedDirective::Digipeat),
            Some("0 1 WIDE1-1,WIDE2-1 TRACE")
        );
        assert_eq!(
            doc.serialize(),
            "# rig notes\nDIGIPEAT 0 1 WIDE1-1,WIDE2-1 TRACE\n\nADEVICE plughw:0,0\n"
        );
    }
}
