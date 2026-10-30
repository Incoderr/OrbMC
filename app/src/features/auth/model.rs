use serde::{Deserialize, Serialize};

/// An Ely.by account the launcher is signed in to. The access token is kept
/// in the OS credential store under [`ElyAccount::token_key`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElyAccount {
    /// Profile UUID without dashes, as Ely.by returns it.
    pub uuid: String,
    pub name: String,
    /// Identifies this launcher to Ely.by; refreshing a token needs it.
    pub client_token: String,
}

impl ElyAccount {
    pub(super) fn token_key(&self) -> String {
        token_key(&self.uuid)
    }
}

pub(super) fn token_key(uuid: &str) -> String {
    format!("ely.by:{uuid}")
}

/// `accounts.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct AccountsFile {
    #[serde(default)]
    pub ely: Vec<ElyAccount>,
    /// UUID of the Ely.by account that plays; `None` is offline mode.
    #[serde(default)]
    pub selected: Option<String>,
}

/// Who a launch is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerAccount {
    Offline { name: String },
    ElyBy(ElyAccount),
}
