use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

use super::{error::ValidationError, validate_identifier};

/// A stable key for keyset pagination. The ID breaks ties when timestamps are
/// equal, so records are neither duplicated nor skipped between pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaginationPosition {
    pub created_at: DateTime<Utc>,
    pub id: String,
}

impl PaginationPosition {
    pub fn new(created_at: DateTime<Utc>, id: impl AsRef<str>) -> Self {
        Self {
            created_at,
            id: id.as_ref().to_string(),
        }
    }

    pub fn try_new(
        created_at: DateTime<Utc>,
        id: impl AsRef<str>,
    ) -> Result<Self, ValidationError> {
        Ok(Self {
            created_at,
            id: validate_identifier(id.as_ref())?,
        })
    }

    pub fn is_after(&self, other: &Self) -> bool {
        self > other
    }
}

impl Ord for PaginationPosition {
    fn cmp(&self, other: &Self) -> Ordering {
        self.created_at
            .cmp(&other.created_at)
            .then_with(|| self.id.cmp(&other.id))
    }
}

impl PartialOrd for PaginationPosition {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn equal_timestamps_are_ordered_by_id() {
        let timestamp = Utc.timestamp_opt(10, 0).unwrap();
        let first = PaginationPosition::new(timestamp, "a");
        let second = PaginationPosition::new(timestamp, "b");
        assert!(second.is_after(&first));
        assert!(!first.is_after(&second));
    }

    #[test]
    fn cursor_boundaries_are_strict() {
        let timestamp = Utc.timestamp_opt(10, 0).unwrap();
        let cursor = PaginationPosition::new(timestamp, "a");
        assert!(!cursor.is_after(&cursor));
        assert!(PaginationPosition::new(timestamp, "b").is_after(&cursor));
        assert!(PaginationPosition::new(Utc.timestamp_opt(11, 0).unwrap(), "a").is_after(&cursor));
    }
}
