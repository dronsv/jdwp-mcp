// Debug session management
//
// Manages JDWP connection state, breakpoints, and thread tracking

use jdwp_client::{EventSet, JdwpConnection};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;

pub type SessionId = String;

#[derive(Debug)]
pub struct DebugSession {
    pub connection: JdwpConnection,
    pub breakpoints: HashMap<String, BreakpointInfo>,
    pub threads: HashMap<String, ThreadInfo>,
    pub class_signatures: HashMap<u64, String>,
    pub active_step: Option<StepRequestInfo>,
    pub selected_thread_id: Option<u64>,
    pub last_event: Option<EventSet>,
    pub last_event_seq: u64,
    pub last_event_notify: Arc<Notify>,
    pub event_listener_task: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone)]
pub struct BreakpointInfo {
    pub id: String,
    pub request_id: i32,
    pub class_pattern: String,
    pub line: u32,
    pub method: Option<String>,
    pub enabled: bool,
    pub hit_count: u32,
    /// If set, variable name and expected value for server-side filtering.
    /// Format: "var_name==value" — auto-resumes if condition is false.
    pub condition: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ThreadInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub suspended: bool,
}

#[derive(Debug, Clone)]
pub struct StepRequestInfo {
    pub request_id: i32,
    pub thread_id: u64,
    pub depth: String,
}

struct SessionState {
    sessions: HashMap<SessionId, Arc<Mutex<DebugSession>>>,
    current: Option<SessionId>,
}

#[derive(Clone)]
pub struct SessionManager {
    state: Arc<Mutex<SessionState>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(SessionState {
                sessions: HashMap::new(),
                current: None,
            })),
        }
    }

    pub async fn create_session(&self, connection: JdwpConnection) -> SessionId {
        let session_id = format!("session_{}", uuid::v4());
        let session = DebugSession {
            connection,
            breakpoints: HashMap::new(),
            threads: HashMap::new(),
            class_signatures: HashMap::new(),
            active_step: None,
            selected_thread_id: None,
            last_event: None,
            last_event_seq: 0,
            last_event_notify: Arc::new(Notify::new()),
            event_listener_task: None,
        };

        let mut state = self.state.lock().await;
        state
            .sessions
            .insert(session_id.clone(), Arc::new(Mutex::new(session)));
        state.current = Some(session_id.clone());

        session_id
    }

    pub async fn get_current_session(&self) -> Option<Arc<Mutex<DebugSession>>> {
        let state = self.state.lock().await;
        let session_id = state.current.as_ref()?;
        state.sessions.get(session_id).cloned()
    }

    pub async fn get_current_session_id(&self) -> Option<SessionId> {
        let state = self.state.lock().await;
        state.current.clone()
    }

    pub async fn remove_session(&self, session_id: &str) {
        let mut state = self.state.lock().await;

        // Abort the event listener task if it exists
        if let Some(session_arc) = state.sessions.get(session_id) {
            let mut session = session_arc.lock().await;
            if let Some(task) = session.event_listener_task.take() {
                task.abort();
            }
        }

        state.sessions.remove(session_id);

        // Clear current if it was this session
        if state.current.as_deref() == Some(session_id) {
            state.current = None;
        }
    }
}

// Simple UUID generation for session IDs
mod uuid {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(1);

    pub fn v4() -> String {
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();

        format!("{:x}{:x}", timestamp, counter)
    }
}
