use gpui_kit::*;

use super::ely::{self, AuthError, Session};
use super::model::{AccountsFile, ElyAccount, PlayerAccount};
use crate::platform::{credentials, profile_store};

/// Signed-in Ely.by accounts and which account plays.
pub struct AuthState {
    accounts: AccountsFile,
}

impl AuthState {
    pub fn load() -> Self {
        let mut accounts: AccountsFile = profile_store::load_accounts().unwrap_or_else(|error| {
            tracing::warn!(%error, "failed to read accounts.json");
            AccountsFile::default()
        });
        if let Some(selected) = &accounts.selected
            && !accounts.ely.iter().any(|account| &account.uuid == selected)
        {
            accounts.selected = None;
        }
        Self { accounts }
    }

    pub fn ely_accounts(&self) -> &[ElyAccount] {
        &self.accounts.ely
    }

    /// The Ely.by account that plays, or `None` in offline mode.
    pub fn selected(&self) -> Option<&ElyAccount> {
        let selected = self.accounts.selected.as_ref()?;
        self.accounts
            .ely
            .iter()
            .find(|account| &account.uuid == selected)
    }

    pub fn player_account(&self, offline_name: &str) -> PlayerAccount {
        match self.selected() {
            Some(account) => PlayerAccount::ElyBy(account.clone()),
            None => PlayerAccount::Offline {
                name: offline_name.to_owned(),
            },
        }
    }

    /// Plays as `uuid`, or offline for `None`.
    pub fn select(&mut self, uuid: Option<String>, cx: &mut Context<Self>) {
        self.accounts.selected = uuid;
        self.save();
        cx.notify();
    }

    /// Keeps a fresh sign-in and makes it the playing account. The token is
    /// already stored by [`sign_in`].
    pub(super) fn add(&mut self, account: ElyAccount, cx: &mut Context<Self>) {
        let uuid = account.uuid.clone();
        match self
            .accounts
            .ely
            .iter_mut()
            .find(|known| known.uuid == uuid)
        {
            Some(known) => *known = account,
            None => self.accounts.ely.push(account),
        }
        self.select(Some(uuid), cx);
    }

    /// Forgets the account here and ends its session on Ely.by.
    pub fn sign_out(&mut self, uuid: &str, cx: &mut Context<Self>) {
        let Some(index) = self
            .accounts
            .ely
            .iter()
            .position(|account| account.uuid == uuid)
        else {
            return;
        };
        let account = self.accounts.ely.remove(index);
        if self.accounts.selected.as_deref() == Some(uuid) {
            self.accounts.selected = None;
        }
        // The credential store and the network both block.
        cx.background_executor()
            .spawn(async move {
                let token = credentials::load(&account.token_key()).ok().flatten();
                if let Err(error) = credentials::delete(&account.token_key()) {
                    tracing::warn!(%error, "failed to delete an Ely.by token");
                }
                if let Some(token) = token
                    && let Err(error) = ely::invalidate(&account, &token)
                {
                    tracing::warn!(%error, "failed to end the Ely.by session");
                }
            })
            .detach();
        self.save();
        cx.notify();
    }

    fn save(&self) {
        if let Err(error) = profile_store::save_accounts(&self.accounts) {
            tracing::error!(%error, "failed to save accounts.json");
        }
    }
}

/// Signs in and stores the token. Blocks; run it off the UI thread.
pub(super) fn sign_in(
    login: &str,
    password: &str,
    totp: Option<&str>,
) -> Result<ElyAccount, AuthError> {
    let Session {
        account,
        access_token,
    } = ely::authenticate(login, password, totp)?;
    credentials::store(&account.token_key(), &access_token)?;
    Ok(account)
}
