//! Provider traits for dependency injection
//!
//! These traits define what types can be injected and how they behave.

use std::any::TypeId;
use std::sync::Arc;

/// Marker trait for types that can be injected via the DI container.
///
/// This is automatically implemented for all types that are `Send + Sync + 'static`.
/// You never need to implement this manually.
///
/// # Examples
///
/// ```rust
/// // Any type that is Send + Sync + 'static works automatically
/// #[derive(Clone)]
/// struct MyService {
///     name: String,
/// }
///
/// // No impl needed - it just works!
/// ```
pub trait Injectable: Send + Sync + 'static {
    /// Returns the TypeId of this type (for internal use)
    #[inline]
    fn type_id_of() -> TypeId
    where
        Self: Sized,
    {
        TypeId::of::<Self>()
    }

    /// Returns the type name for debugging
    #[inline]
    fn type_name_of() -> &'static str
    where
        Self: Sized,
    {
        std::any::type_name::<Self>()
    }
}

// Blanket implementation - everything that's Send + Sync + 'static is Injectable
impl<T: Send + Sync + 'static> Injectable for T {}

/// Backward compatibility alias
pub trait Provider: Injectable {}
impl<T: Injectable> Provider for T {}

/// Service lifetime specification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Lifetime {
    /// Single instance shared across all resolves
    #[default]
    Singleton,

    /// New instance created lazily on first access, then shared
    Lazy,

    /// New instance created on every resolve
    Transient,

    /// One instance per scope
    Scoped,
}

/// Registration information for a provider (used by module system)
#[derive(Clone)]
pub struct ProviderRegistration {
    /// TypeId of the provider
    pub type_id: TypeId,
    /// Human-readable type name
    pub type_name: &'static str,
    /// Registration function
    pub register_fn: Arc<dyn Fn(&crate::Container) + Send + Sync>,
}

impl ProviderRegistration {
    /// Create a new registration for type T
    #[inline]
    pub fn new<T: Injectable>(register_fn: fn(&crate::Container)) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: std::any::type_name::<T>(),
            register_fn: Arc::new(register_fn),
        }
    }

    /// Create from a singleton value
    ///
    /// The value is captured by the registration, and `register_fn` registers
    /// a clone of it as a singleton on the container it is given.
    pub fn singleton<T: Injectable + Clone>(value: T) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: std::any::type_name::<T>(),
            register_fn: Arc::new(move |container: &crate::Container| {
                container.singleton(value.clone());
            }),
        }
    }
}

impl std::fmt::Debug for ProviderRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRegistration")
            .field("type_id", &self.type_id)
            .field("type_name", &self.type_name)
            .finish()
    }
}

/// Helper macro to create a provider registration
#[macro_export]
macro_rules! provider {
    ($type:ty, $factory:expr) => {
        $crate::ProviderRegistration {
            type_id: std::any::TypeId::of::<$type>(),
            type_name: std::any::type_name::<$type>(),
            register_fn: $crate::Arc::new(|container: &$crate::Container| {
                container.singleton($factory);
            }),
        }
    };
    (lazy $type:ty, $factory:expr) => {
        $crate::ProviderRegistration {
            type_id: std::any::TypeId::of::<$type>(),
            type_name: std::any::type_name::<$type>(),
            register_fn: $crate::Arc::new(|container: &$crate::Container| {
                container.lazy($factory);
            }),
        }
    };
    (transient $type:ty, $factory:expr) => {
        $crate::ProviderRegistration {
            type_id: std::any::TypeId::of::<$type>(),
            type_name: std::any::type_name::<$type>(),
            register_fn: $crate::Arc::new(|container: &$crate::Container| {
                container.transient($factory);
            }),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Container;

    #[derive(Clone)]
    struct TestService {
        value: String,
    }

    #[test]
    fn test_singleton_registration_registers_value() {
        let registration = ProviderRegistration::singleton(TestService {
            value: "provided".into(),
        });

        assert_eq!(registration.type_id, TypeId::of::<TestService>());

        // Applying the registration actually registers the captured value
        let container = Container::new();
        (registration.register_fn)(&container);

        let service = container.get::<TestService>().unwrap();
        assert_eq!(service.value, "provided");
    }
}
