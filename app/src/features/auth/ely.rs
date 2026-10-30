//! Ely.by auth server (Yggdrasil protocol): sign-in, token checks and refresh.
//! All calls block; run them off the UI thread.

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use super::model::ElyAccount;
use crate::platform::credentials::{self, CredentialError};

const AUTH_SERVER: &str = "https://authserver.ely.by/auth";
const USER_AGENT: &str = concat!("OrbMC/", env!("CARGO_PKG_VERSION"));

/// A signed-in Ely.by profile with a fresh access token.
#[derive(Debug, Clone)]
pub struct Session {
    pub account: ElyAccount,
    pub access_token: String,
}

/// Why signing in or refreshing a session failed. The messages are shown to
/// the user.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Неверный логин или пароль")]
    InvalidCredentials,
    #[error("Введите код двухфакторной аутентификации")]
    TwoFactorRequired,
    #[error("Сессия Ely.by истекла, войдите в аккаунт снова")]
    SessionExpired,
    #[error("На аккаунте Ely.by нет игрового профиля")]
    NoProfile,
    #[error("Ely.by: {0}")]
    Server(String),
    #[error("Не удалось связаться с Ely.by: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Не удалось скачать authlib-injector: {0}")]
    Injector(String),
    #[error(transparent)]
    Credentials(#[from] CredentialError),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenResponse {
    access_token: String,
    client_token: String,
    selected_profile: Option<Profile>,
}

#[derive(Deserialize)]
struct Profile {
    id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ErrorResponse {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_message: String,
}

#[derive(Clone, Copy)]
enum Call {
    Authenticate,
    Refresh,
}

/// Signs in with a nickname or e-mail. Accounts with two-factor auth need
/// `totp`; without it the call fails with [`AuthError::TwoFactorRequired`].
pub fn authenticate(login: &str, password: &str, totp: Option<&str>) -> Result<Session, AuthError> {
    // Ely.by takes the one-time code appended to the password.
    let password = match totp.map(str::trim).filter(|code| !code.is_empty()) {
        Some(code) => format!("{password}:{code}"),
        None => password.to_owned(),
    };
    let client_token = uuid::Uuid::new_v4().simple().to_string();
    let body = json!({
        "username": login.trim(),
        "password": password,
        "clientToken": client_token,
        "requestUser": true,
    });
    token_call("authenticate", &body, Call::Authenticate)
}

/// A session ready for launching `account`: the stored token if Ely.by still
/// accepts it, otherwise a refreshed one (which is stored in its place).
pub fn launch_session(account: &ElyAccount) -> Result<Session, AuthError> {
    let token = credentials::load(&account.token_key())?.ok_or(AuthError::SessionExpired)?;
    if validate(&token)? {
        return Ok(Session {
            account: account.clone(),
            access_token: token,
        });
    }
    let body = json!({
        "accessToken": token,
        "clientToken": account.client_token,
        "requestUser": true,
    });
    let session = token_call("refresh", &body, Call::Refresh)?;
    credentials::store(&account.token_key(), &session.access_token)?;
    Ok(session)
}

/// Ends the session on Ely.by. Failures are ignored by callers: the local
/// token is dropped either way.
pub fn invalidate(account: &ElyAccount, access_token: &str) -> Result<(), AuthError> {
    let body = json!({ "accessToken": access_token, "clientToken": account.client_token });
    client()?
        .post(format!("{AUTH_SERVER}/invalidate"))
        .json(&body)
        .send()?;
    Ok(())
}

fn validate(access_token: &str) -> Result<bool, AuthError> {
    let response = client()?
        .post(format!("{AUTH_SERVER}/validate"))
        .json(&json!({ "accessToken": access_token }))
        .send()?;
    Ok(response.status().is_success())
}

fn token_call(endpoint: &str, body: &serde_json::Value, call: Call) -> Result<Session, AuthError> {
    let response = client()?
        .post(format!("{AUTH_SERVER}/{endpoint}"))
        .json(body)
        .send()?;
    let status = response.status();
    if !status.is_success() {
        let error = response.json::<ErrorResponse>().unwrap_or(ErrorResponse {
            error: String::new(),
            error_message: format!("HTTP {status}"),
        });
        return Err(classify_error(&error, call));
    }
    let response: TokenResponse = response.json()?;
    let profile = response.selected_profile.ok_or(AuthError::NoProfile)?;
    Ok(Session {
        account: ElyAccount {
            uuid: profile.id,
            name: profile.name,
            client_token: response.client_token,
        },
        access_token: response.access_token,
    })
}

fn classify_error(error: &ErrorResponse, call: Call) -> AuthError {
    let message = error.error_message.to_lowercase();
    if error.error != "ForbiddenOperationException" {
        return AuthError::Server(error.error_message.clone());
    }
    if message.contains("two factor") {
        AuthError::TwoFactorRequired
    } else {
        match call {
            Call::Authenticate => AuthError::InvalidCredentials,
            Call::Refresh => AuthError::SessionExpired,
        }
    }
}

fn client() -> Result<reqwest::blocking::Client, AuthError> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(20))
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forbidden(message: &str) -> ErrorResponse {
        ErrorResponse {
            error: "ForbiddenOperationException".to_owned(),
            error_message: message.to_owned(),
        }
    }

    #[test]
    fn two_factor_is_told_apart_from_a_wrong_password() {
        assert!(matches!(
            classify_error(
                &forbidden("Account protected with two factor auth."),
                Call::Authenticate
            ),
            AuthError::TwoFactorRequired
        ));
        assert!(matches!(
            classify_error(
                &forbidden("Invalid credentials. Invalid nickname or password."),
                Call::Authenticate
            ),
            AuthError::InvalidCredentials
        ));
    }

    #[test]
    fn a_rejected_refresh_means_signing_in_again() {
        assert!(matches!(
            classify_error(&forbidden("Token expired."), Call::Refresh),
            AuthError::SessionExpired
        ));
        let other = ErrorResponse {
            error: "IllegalArgumentException".to_owned(),
            error_message: "credentials can not be null.".to_owned(),
        };
        assert!(matches!(
            classify_error(&other, Call::Refresh),
            AuthError::Server(_)
        ));
    }
}
