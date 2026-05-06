#![allow(
    clippy::too_many_lines,
    clippy::single_char_pattern,
    clippy::use_self,
    clippy::unused_async,
    clippy::redundant_closure,
    clippy::map_identity,
    clippy::derive_partial_eq_without_eq,
    clippy::len_without_is_empty,
    clippy::new_without_default,
    clippy::match_same_arms,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::wildcard_imports,
    clippy::default_trait_access,
    clippy::redundant_clone,
    clippy::significant_drop_in_scrutinee,
    clippy::missing_const_for_fn,
    clippy::unused_self,
    clippy::option_map_or_none,
    clippy::items_after_statements,
    clippy::let_and_return,
    clippy::map_unwrap_or,
    clippy::unnecessary_lazy_evaluations,
    clippy::bool_assert_comparison,
    clippy::option_if_let_else,
    clippy::unnecessary_wraps,
    clippy::if_not_else
)]

pub mod approve;
pub mod cross_platform;
pub mod patterns;
pub mod permission;
pub mod permission_store;
pub mod sandbox;
pub mod trust;
pub mod validation;

#[allow(ambiguous_glob_reexports)]
pub use approve::*;
#[allow(ambiguous_glob_reexports)]
pub use cross_platform::*;
#[allow(ambiguous_glob_reexports)]
pub use patterns::*;
#[allow(ambiguous_glob_reexports)]
pub use permission::*;
#[allow(ambiguous_glob_reexports)]
pub use permission_store::*;
#[allow(ambiguous_glob_reexports)]
pub use sandbox::*;
#[allow(ambiguous_glob_reexports)]
pub use trust::*;
#[allow(ambiguous_glob_reexports)]
pub use validation::*;
