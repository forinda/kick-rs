//! Named injection tokens for trait-object bindings and disambiguation.
//!
//! ```ignore
//! use rustkick_core::Token;
//! pub static USER_REPO: Token<dyn UserRepo> = Token::new("users/repository");
//! ```

use std::any::TypeId;
use std::marker::PhantomData;

/// A typed, named DI token.
///
/// `T: ?Sized` so the token can name a trait object (`dyn Trait`).
pub struct Token<T: ?Sized + 'static> {
    /// Stable name surfaced in error messages and DevTools output.
    pub name: &'static str,
    _marker: PhantomData<fn() -> T>,
}

impl<T: ?Sized + 'static> Token<T> {
    /// Declare a new token with a stable name.
    pub const fn new(name: &'static str) -> Self {
        Self { name, _marker: PhantomData }
    }

    /// `TypeId` of the *pointed-at* type — used for lookup keys.
    pub fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }
}

impl<T: ?Sized + 'static> Clone for Token<T> {
    fn clone(&self) -> Self { *self }
}
impl<T: ?Sized + 'static> Copy for Token<T> {}
