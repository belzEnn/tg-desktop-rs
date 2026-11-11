//! Conversions from TDLib updates into shared application events.

use tdlib_rs::enums::{AuthorizationState as TdAuthorizationState, Update};

use crate::domain::AuthorizationState;

pub(super) fn authorization_state(update: &Update) -> Option<AuthorizationState> {
    let Update::AuthorizationState(update) = update else {
        return None;
    };

    Some(match &update.authorization_state {
        TdAuthorizationState::WaitTdlibParameters => AuthorizationState::WaitTdlibParameters,
        TdAuthorizationState::WaitPhoneNumber => AuthorizationState::WaitPhoneNumber,
        TdAuthorizationState::WaitCode(_) => AuthorizationState::WaitCode,
        TdAuthorizationState::WaitPassword(_) => AuthorizationState::WaitPassword,
        TdAuthorizationState::WaitRegistration(_) => AuthorizationState::WaitRegistration,
        TdAuthorizationState::Ready => AuthorizationState::Ready,
        TdAuthorizationState::Closed => AuthorizationState::Closed,
        _ => AuthorizationState::Other,
    })
}

/// Converts TDLib's error text to a fixed, non-sensitive message for the login UI.
pub(super) fn authorization_error(message: String) -> String {
    match message.as_str() {
        "PHONE_NUMBER_INVALID" => "Enter a valid phone number.".into(),
        "PHONE_NUMBER_BANNED" => "This phone number can't be used to sign in.".into(),
        "PHONE_CODE_INVALID" => "The login code is incorrect. Try again.".into(),
        "PHONE_CODE_EXPIRED" => "The login code expired. Request a new one.".into(),
        "PASSWORD_HASH_INVALID" => "The 2FA password is incorrect. Try again.".into(),
        "FLOOD_WAIT" | "FLOOD" => "Too many attempts. Wait a while and try again.".into(),
        _ => "Telegram couldn't complete sign-in. Check your details and try again.".into(),
    }
}
