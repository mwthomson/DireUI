use std::collections::HashMap;

// A one-time message shown on the Profiles page after a redirect, carried
// via a `flash` query parameter since the app has no session/cookie layer.
#[derive(Debug, Clone, PartialEq)]
pub enum Flash {
    Saved,
    NoChange,
    Removed,
    Deleted,
    DeleteFailed(String),
}

impl Flash {
    pub fn to_query_string(&self) -> String {
        match self {
            Flash::Saved => "flash=saved".to_string(),
            Flash::NoChange => "flash=nochange".to_string(),
            Flash::Removed => "flash=removed".to_string(),
            Flash::Deleted => "flash=deleted".to_string(),
            Flash::DeleteFailed(reason) => {
                format!("flash=delete_failed&reason={}", percent_encode(reason))
            }
        }
    }

    pub fn from_params(params: &HashMap<String, String>) -> Option<Flash> {
        match params.get("flash")?.as_str() {
            "saved" => Some(Flash::Saved),
            "nochange" => Some(Flash::NoChange),
            "removed" => Some(Flash::Removed),
            "deleted" => Some(Flash::Deleted),
            "delete_failed" => Some(Flash::DeleteFailed(
                params.get("reason").cloned().unwrap_or_default(),
            )),
            _ => None,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Flash::Saved => "changes saved".to_string(),
            Flash::NoChange => "no changes to save".to_string(),
            Flash::Removed => "Profile removed".to_string(),
            Flash::Deleted => "Profile and file deleted".to_string(),
            Flash::DeleteFailed(reason) => format!("couldn't delete file: {reason}"),
        }
    }
}

// axum's Query extractor percent-decodes incoming values for us, so this
// only needs to handle the outgoing direction (building a Redirect's URI).
fn percent_encode(input: &str) -> String {
    input
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn saved_round_trips_through_query_string() {
        assert_eq!(Flash::Saved.to_query_string(), "flash=saved");
        assert_eq!(Flash::from_params(&params(&[("flash", "saved")])), Some(Flash::Saved));
    }

    #[test]
    fn nochange_round_trips_through_query_string() {
        assert_eq!(Flash::NoChange.to_query_string(), "flash=nochange");
        assert_eq!(
            Flash::from_params(&params(&[("flash", "nochange")])),
            Some(Flash::NoChange)
        );
    }

    #[test]
    fn removed_round_trips_through_query_string() {
        assert_eq!(Flash::Removed.to_query_string(), "flash=removed");
        assert_eq!(Flash::from_params(&params(&[("flash", "removed")])), Some(Flash::Removed));
    }

    #[test]
    fn deleted_round_trips_through_query_string() {
        assert_eq!(Flash::Deleted.to_query_string(), "flash=deleted");
        assert_eq!(Flash::from_params(&params(&[("flash", "deleted")])), Some(Flash::Deleted));
    }

    #[test]
    fn delete_failed_percent_encodes_the_reason_in_the_query_string() {
        let flash = Flash::DeleteFailed("Permission denied (os error 13)".to_string());

        assert_eq!(
            flash.to_query_string(),
            "flash=delete_failed&reason=Permission%20denied%20%28os%20error%2013%29"
        );
    }

    #[test]
    fn delete_failed_round_trips_the_reason_from_params() {
        let params = params(&[("flash", "delete_failed"), ("reason", "disk full")]);

        assert_eq!(
            Flash::from_params(&params),
            Some(Flash::DeleteFailed("disk full".to_string()))
        );
    }

    #[test]
    fn from_params_returns_none_when_flash_is_absent() {
        assert_eq!(Flash::from_params(&HashMap::new()), None);
    }

    #[test]
    fn from_params_returns_none_for_an_unrecognized_flash_value() {
        assert_eq!(Flash::from_params(&params(&[("flash", "bogus")])), None);
    }

    #[test]
    fn message_text_matches_the_agreed_wording() {
        assert_eq!(Flash::Saved.message(), "changes saved");
        assert_eq!(Flash::NoChange.message(), "no changes to save");
        assert_eq!(Flash::Removed.message(), "Profile removed");
        assert_eq!(Flash::Deleted.message(), "Profile and file deleted");
        assert_eq!(
            Flash::DeleteFailed("disk full".to_string()).message(),
            "couldn't delete file: disk full"
        );
    }
}
