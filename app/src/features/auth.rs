//! Who plays: the offline player name or an Ely.by account. Ely.by sessions
//! reach the game through authlib-injector, which points the game's auth and
//! skin requests at Ely.by.

mod ely;
mod injector;
mod model;
mod state;
mod view;

pub use ely::{AuthError, launch_session};
pub use injector::authlib_injector;
pub use model::PlayerAccount;
pub use state::AuthState;
pub use view::AccountView;

/// authlib-injector's shorthand for the Ely.by API root.
pub const ELY_BY_API: &str = "ely.by";
