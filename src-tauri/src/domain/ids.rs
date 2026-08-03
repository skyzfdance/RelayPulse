use std::fmt;

use uuid::Uuid;

use super::{DomainError, Result};

const MAX_ENTITY_ID_LENGTH: usize = 128;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(String);

impl EntityId {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > MAX_ENTITY_ID_LENGTH {
            return Err(DomainError::invalid("id"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for EntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("EntityId").field(&self.0).finish()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub trait IdGenerator: Send + Sync {
    fn next_id(&self) -> EntityId;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UuidGenerator;

impl IdGenerator for UuidGenerator {
    fn next_id(&self) -> EntityId {
        EntityId(Uuid::new_v4().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_ids_reject_blank_and_oversized_values() {
        assert!(EntityId::parse("  ").is_err());
        assert!(EntityId::parse("x".repeat(MAX_ENTITY_ID_LENGTH + 1)).is_err());
    }

    #[test]
    fn uuid_generator_returns_distinct_parseable_ids() {
        let generator = UuidGenerator;
        let first = generator.next_id();
        let second = generator.next_id();

        assert_ne!(first, second);
        assert!(Uuid::parse_str(first.as_str()).is_ok());
        assert!(Uuid::parse_str(second.as_str()).is_ok());
    }
}
