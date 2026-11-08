//! One configured XMPP account and its runtime objects.
//!
//! An [`AccountSession`] bundles everything that belongs to a single JID: its
//! connection, managers, event bus and the receiver feeding the event
//! processor. Sessions are independent, so several can exist at once; the app
//! currently shows one active session and will connect the others in the
//! background.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::{
    ChatManager, ClientConfig, Connection, ConnectionEvent, DiscoManager, EventBus, IqRouter,
    JingleManager, MamManager, MucManager, OmemoManager, PubSubManager, PushManager, Roster,
    SpaceManager, StreamManagement, VoiceManager, XmppClient,
};

/// Runtime state for one configured account.
pub struct AccountSession {
    /// Bare JID of the account.
    pub account: String,
    /// Configuration used to create the connection.
    pub config: ClientConfig,
    /// Event bus shared by this session's managers.
    pub events: Arc<EventBus>,
    /// Client handle used by the UI and the event processor.
    pub client: XmppClient,
    receiver: Option<mpsc::UnboundedReceiver<ConnectionEvent>>,
}

impl AccountSession {
    /// Build a session with its own event bus.
    #[must_use]
    pub fn new(config: ClientConfig) -> Self {
        Self::with_events(config, Arc::new(EventBus::new()))
    }

    /// Build a session that publishes to an existing event bus.
    ///
    /// The app uses this for the active session so its notification and
    /// persistence loops keep receiving the same stream as before.
    #[must_use]
    pub fn with_events(config: ClientConfig, events: Arc<EventBus>) -> Self {
        let account = config
            .jid
            .split('/')
            .next()
            .unwrap_or(&config.jid)
            .to_string();
        let (connection, receiver) = Connection::new(config.clone().into());
        let connection = Arc::new(connection);
        let roster = Arc::new(Roster::new(events.clone()));
        let chats = Arc::new(ChatManager::new(events.clone()));
        let mucs = Arc::new(MucManager::new(events.clone()));
        let pubsub = Arc::new(PubSubManager::new(events.clone()));
        let spaces = Arc::new(SpaceManager::new(events.clone()));
        let disco = Arc::new(DiscoManager::new());
        let push = Arc::new(PushManager::new());
        let sm = Arc::new(StreamManagement::new());
        let omemo = Arc::new(OmemoManager::new());
        let iq = Arc::new(IqRouter::new(std::time::Duration::from_secs(30)));
        let mam = Arc::new(MamManager::new());
        let jingle = JingleManager::new(connection.clone(), iq.clone(), events.clone());
        let conference = crate::ConferenceManager::new();
        connection.set_stream_management(sm.clone());
        let client = XmppClient {
            connection,
            roster,
            chats,
            mucs,
            pubsub,
            spaces,
            disco,
            push,
            sm,
            omemo,
            events: events.clone(),
            iq,
            mam,
            jingle,
            conference,
            voice: Arc::new(VoiceManager::new()),
        };
        Self {
            account,
            config,
            events,
            client,
            receiver: Some(receiver),
        }
    }

    /// Take the connection event receiver, which can be handed to
    /// [`crate::EventProcessor::start`]. It can only be taken once.
    #[must_use]
    pub const fn take_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<ConnectionEvent>> {
        self.receiver.take()
    }

    /// Connect this account.
    ///
    /// # Errors
    ///
    /// Returns an error when the connection or authentication fails.
    pub async fn connect(&self) -> crate::Result<()> {
        self.client.connection.connect().await
    }

    /// Disconnect this account without clearing its managers.
    pub async fn disconnect(&self) {
        let _ = self.client.connection.disconnect().await;
    }

    /// Whether the underlying connection is connected.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.client.connection.current_state().is_connected()
    }
}

/// Sessions for every configured account, keyed by bare JID.
pub struct SessionRegistry {
    sessions: Mutex<HashMap<String, AccountSession>>,
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Add or replace one session.
    pub fn insert(&self, session: AccountSession) {
        self.sessions
            .lock()
            .insert(session.account.clone(), session);
    }

    /// Remove one session.
    pub fn remove(&self, account: &str) -> Option<AccountSession> {
        self.sessions.lock().remove(account)
    }

    /// Bare JIDs in stable order.
    #[must_use]
    pub fn accounts(&self) -> Vec<String> {
        let mut accounts: Vec<String> = self.sessions.lock().keys().cloned().collect();
        accounts.sort();
        accounts
    }

    #[must_use]
    pub fn client(&self, account: &str) -> Option<XmppClient> {
        self.sessions
            .lock()
            .get(account)
            .map(|session| session.client.clone())
    }

    #[must_use]
    pub fn config(&self, account: &str) -> Option<ClientConfig> {
        self.sessions
            .lock()
            .get(account)
            .map(|session| session.config.clone())
    }

    #[must_use]
    pub fn events(&self, account: &str) -> Option<Arc<EventBus>> {
        self.sessions
            .lock()
            .get(account)
            .map(|session| session.events.clone())
    }

    #[must_use]
    pub fn is_connected(&self, account: &str) -> bool {
        self.sessions
            .lock()
            .get(account)
            .is_some_and(AccountSession::is_connected)
    }

    /// Take the receiver for one session, if it has not been started yet.
    pub fn take_receiver(&self, account: &str) -> Option<mpsc::UnboundedReceiver<ConnectionEvent>> {
        self.sessions
            .lock()
            .get_mut(account)
            .and_then(AccountSession::take_receiver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Jid;

    fn session(jid: &str) -> AccountSession {
        let config = ClientConfig {
            jid: jid.to_string(),
            password: Some("secret".to_string()),
            resource: Some("jabu".to_string()),
            ..ClientConfig::default()
        };
        AccountSession::new(config)
    }

    #[test]
    fn sessions_have_independent_managers_and_buses() {
        let mut alice = session("alice@example.org");
        let bob = session("bob@example.org");
        assert_eq!(alice.account, "alice@example.org");
        assert_eq!(bob.account, "bob@example.org");
        assert!(!Arc::ptr_eq(&alice.events, &bob.events));
        assert!(!Arc::ptr_eq(&alice.client.roster, &bob.client.roster));
        assert!(!Arc::ptr_eq(&alice.client.chats, &bob.client.chats));

        let peer: Jid = "carol@example.org".parse().unwrap();
        alice.client.chats.get_or_create(&peer);
        assert_eq!(alice.client.chats.list().len(), 1);
        assert!(bob.client.chats.list().is_empty());

        assert!(alice.take_receiver().is_some());
        assert!(alice.take_receiver().is_none());
    }

    #[test]
    fn a_session_can_share_an_existing_event_bus() {
        let events = Arc::new(EventBus::new());
        let session = AccountSession::with_events(
            ClientConfig {
                jid: "alice@example.org".to_string(),
                ..ClientConfig::default()
            },
            events.clone(),
        );
        assert!(Arc::ptr_eq(&session.events, &events));
        assert!(Arc::ptr_eq(&session.client.events, &events));
    }

    #[test]
    fn the_registry_indexes_sessions_by_account() {
        let registry = SessionRegistry::new();
        registry.insert(session("bob@example.org"));
        registry.insert(session("alice@example.org"));
        assert_eq!(
            registry.accounts(),
            vec![
                "alice@example.org".to_string(),
                "bob@example.org".to_string()
            ]
        );
        assert!(registry.client("alice@example.org").is_some());
        assert!(registry.config("bob@example.org").is_some());
        assert!(registry.events("alice@example.org").is_some());
        assert!(!registry.is_connected("alice@example.org"));
        assert!(registry.take_receiver("alice@example.org").is_some());
        assert!(registry.take_receiver("alice@example.org").is_none());
        assert!(registry.remove("bob@example.org").is_some());
        assert!(registry.remove("bob@example.org").is_none());
        assert_eq!(registry.accounts(), vec!["alice@example.org".to_string()]);
    }
}
