//! Error types for dependency injection

use std::any::TypeId;
use thiserror::Error;

/// Errors that can occur during dependency injection operations
#[derive(Error, Debug)]
pub enum DiError {
    /// Service was not found in the container
    ///
    /// The container stores services by [`TypeId`] only, so this error can
    /// name the missing type but not the registered ones. Call
    /// `Container::debug_registrations()` on the resolving container to log
    /// what is actually registered in each scope of the chain.
    #[error(
        "Service not found: {type_name} (not registered in this scope chain; \
         was it registered in a different scope, or removed by clear()? \
         Container::debug_registrations() lists what is registered)"
    )]
    NotFound {
        type_name: &'static str,
        type_id: TypeId,
    },

    /// Circular dependency detected during resolution
    #[error("Circular dependency detected while resolving: {type_name}")]
    CircularDependency { type_name: &'static str },

    /// Factory failed to create service
    #[error("Failed to create service {type_name}: {reason}")]
    CreationFailed {
        type_name: &'static str,
        reason: String,
    },

    /// Container is locked and cannot be modified
    #[error("Container is locked - cannot register new services")]
    Locked,

    /// Attempted to register duplicate service
    #[error("Service already registered: {type_name}")]
    AlreadyRegistered { type_name: &'static str },

    /// Parent scope was dropped
    #[error("Parent scope has been dropped")]
    ParentDropped,

    /// Internal error
    #[error("Internal DI error: {0}")]
    Internal(String),
}

impl DiError {
    /// Create a `NotFound` error for a type
    #[inline]
    pub fn not_found<T: 'static>() -> Self {
        Self::NotFound {
            type_name: std::any::type_name::<T>(),
            type_id: TypeId::of::<T>(),
        }
    }

    /// Create a `CreationFailed` error
    #[inline]
    pub fn creation_failed<T: 'static>(reason: impl Into<String>) -> Self {
        Self::CreationFailed {
            type_name: std::any::type_name::<T>(),
            reason: reason.into(),
        }
    }

    /// Create an `AlreadyRegistered` error
    #[inline]
    pub fn already_registered<T: 'static>() -> Self {
        Self::AlreadyRegistered {
            type_name: std::any::type_name::<T>(),
        }
    }

    /// Create a `CircularDependency` error
    #[inline]
    pub fn circular<T: 'static>() -> Self {
        Self::CircularDependency {
            type_name: std::any::type_name::<T>(),
        }
    }
}

impl Clone for DiError {
    fn clone(&self) -> Self {
        match self {
            Self::NotFound { type_name, type_id } => Self::NotFound {
                type_name,
                type_id: *type_id,
            },
            Self::CircularDependency { type_name } => Self::CircularDependency { type_name },
            Self::CreationFailed { type_name, reason } => Self::CreationFailed {
                type_name,
                reason: reason.clone(),
            },
            Self::Locked => Self::Locked,
            Self::AlreadyRegistered { type_name } => Self::AlreadyRegistered { type_name },
            Self::ParentDropped => Self::ParentDropped,
            Self::Internal(s) => Self::Internal(s.clone()),
        }
    }
}

/// Result type alias for DI operations
pub type Result<T> = std::result::Result<T, DiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    struct MissingService;

    #[test]
    fn test_not_found_message_contains_type_name_and_hint() {
        let err = DiError::not_found::<MissingService>();
        let message = err.to_string();

        // Names the missing type
        assert!(message.contains("MissingService"));
        // Explains the likely causes
        assert!(message.contains("different scope"));
        assert!(message.contains("clear()"));
        // Points at the diagnostic helper
        assert!(message.contains("Container::debug_registrations()"));
    }

    #[test]
    fn test_not_found_carries_type_id() {
        let err = DiError::not_found::<MissingService>();
        match err {
            DiError::NotFound { type_id, .. } => {
                assert_eq!(type_id, TypeId::of::<MissingService>());
            }
            other => panic!("expected NotFound, got: {other}"),
        }
    }
}
