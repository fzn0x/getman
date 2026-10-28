//! Signing in to OAuth 2.0 with a browser, from the auth pane: the same
//! steps as the browser UI's sign-in. Getman listens for the provider's
//! callback, opens the authorization URL, and keeps the token it gets.

use std::time::Duration;

use getman_core::auth::{Authorization, Token};
use getman_core::execute::{Cancel, Executor};
use getman_core::http::Transport;
use getman_core::model::OAuth2;
use getman_core::vars::VarStack;
use tokio::sync::mpsc::UnboundedSender;

use super::app::Message;

/// How long a browser sign-in may take.
const TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Opens an address in the system's browser. Tells whether it could.
pub(crate) type Opener = fn(&str) -> bool;

/// Opens the system's browser.
pub(crate) fn open_browser(url: &str) -> bool {
    open::that_detached(url).is_ok()
}

/// What happens during a sign-in.
#[derive(Debug)]
pub(crate) enum Event {
    /// Getman listens for the callback, and the browser was sent to the
    /// authorization URL, or could not be.
    Waiting {
        authorization: Authorization,
        opened: bool,
    },
    /// The sign-in is over: here is the token, or why there is none.
    Done(Result<Token, String>),
}

/// A sign-in under way.
#[derive(Debug)]
pub(crate) struct SignIn {
    /// Tells this sign-in's events from an older one's.
    pub(crate) serial: u64,
    cancel: Cancel,
    /// Known once Getman listens for the callback.
    pub(crate) authorization: Option<Authorization>,
    /// Whether the browser opened.
    pub(crate) opened: bool,
}

impl SignIn {
    /// Starts signing in. The outcome arrives as messages.
    pub(crate) fn start(
        serial: u64,
        executor: &Executor,
        settings: OAuth2,
        vars: VarStack,
        transport: Transport,
        opener: Opener,
        tx: UnboundedSender<Message>,
    ) -> Self {
        let cancel = Cancel::new();
        let executor = executor.clone();
        let stop = cancel.clone();
        tokio::spawn(async move {
            let send = |event| {
                let _ = tx.send(Message::SignIn(serial, event));
            };
            let authorization = match executor
                .authenticator
                .authorize(&settings, &vars, transport)
                .await
            {
                Ok(authorization) => authorization,
                Err(e) => return send(Event::Done(Err(e.to_string()))),
            };
            if stop.is_cancelled() {
                executor
                    .authenticator
                    .cancel_authorization(&authorization.id);
                return;
            }
            let id = authorization.id.clone();
            let shown = opener(&authorization.url);
            send(Event::Waiting {
                authorization,
                opened: shown,
            });
            let token = executor
                .authenticator
                .wait_for_token(&executor.http, &id, TIMEOUT)
                .await;
            send(Event::Done(token.map_err(|e| e.to_string())));
        });
        Self {
            serial,
            cancel,
            authorization: None,
            opened: false,
        }
    }

    /// Stops waiting for the browser.
    pub(crate) fn cancel(&self, executor: &Executor) {
        self.cancel.cancel();
        if let Some(authorization) = &self.authorization {
            executor
                .authenticator
                .cancel_authorization(&authorization.id);
        }
    }
}

/// What the auth pane knows about the OAuth 2.0 settings the open request
/// uses, itself or through its folders.
#[derive(Debug, Clone)]
pub(crate) struct Credentials {
    /// Whether the grant signs in with a browser.
    pub(crate) browser: bool,
    /// The token kept for these settings.
    pub(crate) token: Option<Token>,
}

/// A token's state in a sentence: whether there is one and when it ends.
pub(crate) fn describe(token: &Token, now_ms: i64) -> String {
    match token.expires_at {
        None => "Signed in · the provider gave no expiry".to_owned(),
        Some(at) if at > now_ms => format!(
            "Signed in · expires in {} ({})",
            span(at - now_ms),
            clock(at)
        ),
        Some(at) => {
            let mut line = format!("Token expired {} ago ({})", span(now_ms - at), clock(at));
            if token.refreshable {
                line.push_str("; it renews when a request needs it");
            }
            line
        }
    }
}

/// A length of time, roughly: `20 s`, `59 min`, `2 h 5 min`, `3 d 4 h`.
fn span(ms: i64) -> String {
    let secs = ms.max(0) / 1000;
    if secs < 60 {
        format!("{secs} s")
    } else if secs < 3600 {
        format!("{} min", secs / 60)
    } else if secs < 86_400 {
        format!("{} h {} min", secs / 3600, secs % 3600 / 60)
    } else {
        format!("{} d {} h", secs / 86_400, secs % 86_400 / 3600)
    }
}

/// A moment as `2026-10-28 14:32 UTC`.
fn clock(ms: i64) -> String {
    let iso = getman_core::time::iso8601_millis(ms);
    format!("{} {} UTC", &iso[..10], &iso[11..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(expires_at: Option<i64>, refreshable: bool) -> Token {
        Token {
            access_token: "t".into(),
            token_type: "Bearer".into(),
            expires_in: None,
            scope: None,
            expires_at,
            refreshable,
        }
    }

    #[test]
    fn a_token_says_when_it_ends() {
        let now = 1_792_000_000_000;
        assert_eq!(
            describe(&token(Some(now + 3_540_000), false), now),
            format!("Signed in · expires in 59 min ({})", clock(now + 3_540_000))
        );
        assert!(
            describe(&token(Some(now + 7_500_000), true), now).contains("2 h 5 min"),
            "hours"
        );
        let past = describe(&token(Some(now - 20_000), true), now);
        assert!(past.starts_with("Token expired 20 s ago ("), "{past}");
        assert!(
            past.ends_with("it renews when a request needs it"),
            "{past}"
        );
        assert!(describe(&token(None, false), now).contains("no expiry"));
        assert_eq!(clock(1_792_000_000_000), "2026-10-14 17:46 UTC");
    }
}
