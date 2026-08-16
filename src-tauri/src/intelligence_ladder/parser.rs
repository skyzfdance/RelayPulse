use std::{cmp::Ordering, collections::HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use unicode_segmentation::UnicodeSegmentation;

use super::IntelligenceRankingEntry;

const MIN_ENTRY_COUNT: usize = 3;
const MAX_ENTRY_COUNT: usize = 100;
const MAX_MODEL_GRAPHEMES: usize = 120;
const MAX_EFFORT_GRAPHEMES: usize = 40;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RawRankingEntry {
    pub model: String,
    pub effort: String,
    pub iq: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum IntelligenceParseError {
    #[error("the ranking entry count is invalid")]
    InvalidEntryCount,
    #[error("a ranking text field is invalid")]
    InvalidText,
    #[error("a ranking IQ value is invalid")]
    InvalidIq,
    #[error("a ranking identity is duplicated")]
    DuplicateIdentity,
}

pub fn parse_rankings(
    entries: Vec<RawRankingEntry>,
) -> Result<Vec<IntelligenceRankingEntry>, IntelligenceParseError> {
    if !(MIN_ENTRY_COUNT..=MAX_ENTRY_COUNT).contains(&entries.len()) {
        return Err(IntelligenceParseError::InvalidEntryCount);
    }

    let mut normalized = Vec::with_capacity(entries.len());
    let mut identities = HashSet::with_capacity(entries.len());
    for entry in entries {
        let model = normalize_text(&entry.model, MAX_MODEL_GRAPHEMES)?;
        let effort = normalize_text(&entry.effort, MAX_EFFORT_GRAPHEMES)?;
        if !entry.iq.is_finite() || !(0.0..=200.0).contains(&entry.iq) {
            return Err(IntelligenceParseError::InvalidIq);
        }
        let identity = format!("{}\u{0}{}", model.to_lowercase(), effort.to_lowercase());
        if !identities.insert(identity) {
            return Err(IntelligenceParseError::DuplicateIdentity);
        }
        normalized.push((model, effort, entry.iq));
    }

    normalized.sort_by(|left, right| {
        right
            .2
            .partial_cmp(&left.2)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.0.to_lowercase().cmp(&right.0.to_lowercase()))
            .then_with(|| left.1.to_lowercase().cmp(&right.1.to_lowercase()))
    });

    normalized
        .into_iter()
        .enumerate()
        .map(|(index, (model, effort, iq))| {
            let rank =
                u32::try_from(index + 1).map_err(|_| IntelligenceParseError::InvalidEntryCount)?;
            Ok(IntelligenceRankingEntry {
                rank,
                model,
                effort,
                iq,
            })
        })
        .collect()
}

fn normalize_text(value: &str, max_graphemes: usize) -> Result<String, IntelligenceParseError> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let grapheme_count = normalized.graphemes(true).count();
    if grapheme_count == 0
        || grapheme_count > max_graphemes
        || normalized
            .chars()
            .any(|character| character.is_control() || matches!(character, '<' | '>'))
    {
        return Err(IntelligenceParseError::InvalidText);
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(model: &str, effort: &str, iq: f64) -> RawRankingEntry {
        RawRankingEntry {
            model: model.to_owned(),
            effort: effort.to_owned(),
            iq,
        }
    }

    #[test]
    fn normalizes_sorts_and_ranks_with_deterministic_ties() {
        let result = parse_rankings(vec![
            entry(" Model B ", " high ", 90.0),
            entry("Model A", "xhigh", 101.5),
            entry("Model A", " high ", 90.0),
        ])
        .expect("fixture is valid");

        assert_eq!(
            result,
            vec![
                IntelligenceRankingEntry {
                    rank: 1,
                    model: "Model A".to_owned(),
                    effort: "xhigh".to_owned(),
                    iq: 101.5,
                },
                IntelligenceRankingEntry {
                    rank: 2,
                    model: "Model A".to_owned(),
                    effort: "high".to_owned(),
                    iq: 90.0,
                },
                IntelligenceRankingEntry {
                    rank: 3,
                    model: "Model B".to_owned(),
                    effort: "high".to_owned(),
                    iq: 90.0,
                },
            ]
        );
    }

    #[test]
    fn rejects_partial_duplicate_and_untrusted_snapshots() {
        assert_eq!(
            parse_rankings(vec![entry("A", "high", 1.0)]),
            Err(IntelligenceParseError::InvalidEntryCount)
        );
        assert_eq!(
            parse_rankings(vec![
                entry("A", "high", 1.0),
                entry("a", "HIGH", 2.0),
                entry("B", "high", 3.0),
            ]),
            Err(IntelligenceParseError::DuplicateIdentity)
        );
        assert_eq!(
            parse_rankings(vec![
                entry("<script>", "high", 1.0),
                entry("B", "high", 2.0),
                entry("C", "high", 3.0),
            ]),
            Err(IntelligenceParseError::InvalidText)
        );
        assert_eq!(
            parse_rankings(vec![
                entry("A", "high", f64::INFINITY),
                entry("B", "high", 2.0),
                entry("C", "high", 3.0),
            ]),
            Err(IntelligenceParseError::InvalidIq)
        );
    }
}
