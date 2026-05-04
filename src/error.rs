//! Error types for dependency injection

use std::any::TypeId;
use thiserror::Error;

/// Errors that can occur during dependency injection operations
#[derive(Error, Debug)]
pub enum DiError {
    /// Service was not found in the container
    #[error("Service not found: {type_name}")]
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
