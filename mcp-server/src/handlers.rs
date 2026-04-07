// MCP request handlers
//
// Handles initialize, list tools, and debug tool execution

use crate::protocol::*;
use crate::session::{SessionManager, StepRequestInfo};
use crate::tools;
use jdwp_client::vm::ClassInfo;
use serde_json::json;
use tracing::{debug, info, warn};

const AGENT_INSTRUCTIONS: &str = r#"JDWP debugger for live Java/JVM applications.

SETUP: The target JVM must be started with JDWP enabled:
  java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005 -jar app.jar
For Spring Boot: mvn spring-boot:run -Dspring-boot.run.jvmArguments="-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005"
For Docker: set env JAVA_TOOL_OPTIONS="-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005" and expose port 5005.
For Kubernetes: kubectl port-forward pod/<name> 5005:5005

WORKFLOW — start every session with:
  1. debug.attach (host, port) — connect to the JVM
  2. Choose a strategy based on the problem:

STRATEGY A — Diagnosing a hang or deadlock:
  debug.pause → debug.list_threads → debug.get_stack (on blocked threads) → debug.inspect (lock objects)

STRATEGY B — Breakpoint-driven debugging:
  debug.find_class (pattern) → debug.list_methods (class) → debug.set_breakpoint (class, line) → debug.wait_for_event → debug.get_stack / debug.get_variable

STRATEGY C — Exception debugging:
  debug.exception_breakpoint (class_pattern, caught, uncaught) → debug.wait_for_event → debug.get_stack

STRATEGY D — Field watchpoint:
  debug.watch (class, field) → debug.wait_for_event → debug.get_stack (see who modified the field)

STRATEGY E — Request path tracing (best for "which code path did my request take?"):
  debug.trace (class_pattern) → send HTTP request to app → debug.trace_result
  Shows the exact call sequence with depth: which methods were called and in what order.
  Use this when a breakpoint doesn't hit and you don't know which path the request took.

KEY TIPS:
- debug.snapshot gives event + breakpoints + stack in one call — use it after any stop event
- debug.get_stack auto-resolves object fields (shows ClassName{field=val} not just @hex)
- debug.eval calls toString() or any no-arg method on an object — thread must be suspended
- debug.set_breakpoint supports condition="var==value" for server-side filtering
- debug.inspect shows all fields of an object by hex ID (from stack output @hex references)
- Always debug.disconnect when done to release the JVM

DEBUGGING APP STARTUP (suspend=y):
If you need to debug code that runs during initialization, start the JVM with suspend=y:
  java -agentlib:jdwp=transport=dt_socket,server=y,suspend=y,address=*:5005 -jar app.jar
The JVM will freeze immediately and wait for a debugger. Then:
  debug.attach → debug.set_breakpoint (set breakpoints before any code runs) → debug.continue (JVM starts)
Use this when the bug happens during startup and you can't set breakpoints fast enough with suspend=n.

COMMON ERRORS:
- "Connection refused" → JVM not started with JDWP flags, or wrong port
- "THREAD_NOT_SUSPENDED" → use debug.pause or hit a breakpoint before debug.eval
- "No method found at line X" → use debug.list_methods to find valid line ranges

EXAMPLE 1 — "API returns wrong data, find why":
  debug.attach localhost:5005
  debug.trace class_pattern=com.example.service    ← arm tracing
  [send the HTTP request that returns wrong data]
  debug.trace_result                               ← see actual call path
  → trace shows: OrderService.getOrder → PriceCalculator.apply → DiscountRule.evaluate
  debug.set_breakpoint class=DiscountRule line=42  ← now you know where to look
  [send the request again]
  debug.wait_for_event
  debug.get_stack                                  ← see variables at decision point
  → found: discountPercent=0.5 but expected 0.1, wrong rule matched

EXAMPLE 2 — "App hangs on certain requests":
  debug.attach localhost:5005
  debug.pause                                      ← freeze everything
  debug.list_threads                               ← find stuck threads
  → thread pool-3-thread-7: suspended
  debug.get_stack thread_id=0x...
  → #0 Database.query:218 sql="SELECT ... WHERE id=?"
  → #1 UserRepository.findById:45 locked on monitor @3f2a
  debug.inspect object_id=0x3f2a                   ← what is the lock object?
  → ConnectionPool{activeCount=50, maxSize=50}     ← pool exhausted!

EXAMPLE 3 — "Exception during startup, need to catch it early":
  [start app]: java -agentlib:jdwp=...,suspend=y -jar app.jar
  debug.attach localhost:5005                      ← JVM is frozen, waiting
  debug.exception_breakpoint class_pattern=NullPointerException
  debug.continue                                   ← let JVM start
  debug.wait_for_event                             ← catches the NPE
  debug.get_stack                                  ← see where it happened
  → #0 ConfigLoader.loadProperties:89 config=null  ← config file missing"#;

pub struct RequestHandler {
    session_manager: SessionManager,
}

impl RequestHandler {
    fn signature_to_binary_name(signature: &str) -> Option<String> {
        if signature.starts_with('L') && signature.ends_with(';') {
            Some(signature[1..signature.len() - 1].replace('/', "."))
        } else {
            None
        }
    }

    fn signature_to_display_name(signature: &str) -> Option<String> {
        Self::signature_to_binary_name(signature).map(|name| name.replace('$', "."))
    }

    fn normalize_class_pattern(pattern: &str) -> String {
        if pattern.starts_with('L') && pattern.ends_with(';') {
            Self::signature_to_binary_name(pattern).unwrap_or_else(|| pattern.to_string())
        } else {
            pattern.replace('/', ".")
        }
    }

    fn candidate_signatures(pattern: &str) -> Vec<String> {
        let mut candidates = Vec::new();
        if pattern.starts_with('L') && pattern.ends_with(';') {
            candidates.push(pattern.to_string());
        } else {
            let binary = pattern.replace('.', "/");
            candidates.push(format!("L{};", binary));
            if pattern.contains('/') {
                candidates.push(format!("L{};", pattern));
            }
        }
        candidates.sort();
        candidates.dedup();
        candidates
    }

    fn class_match_score(signature: &str, pattern: &str) -> Option<u8> {
        let requested = Self::normalize_class_pattern(pattern);
        let binary = Self::signature_to_binary_name(signature)?;
        let display = Self::signature_to_display_name(signature)?;
        let requested_simple = requested.rsplit('.').next().unwrap_or(&requested);
        let binary_simple = binary.rsplit('.').next().unwrap_or(&binary);
        let display_simple = display.rsplit('.').next().unwrap_or(&display);

        if requested == binary || requested == display {
            return Some(0);
        }
        if binary == requested.replace('.', "$") {
            return Some(1);
        }
        if binary.starts_with(&(requested.clone() + "$"))
            || display.starts_with(&(requested.clone() + "."))
        {
            return Some(2);
        }
        if requested_simple == binary_simple || requested_simple == display_simple {
            return Some(3);
        }
        if binary_simple.starts_with(&(requested_simple.to_string() + "$"))
            || display_simple.starts_with(&(requested_simple.to_string() + "."))
        {
            return Some(4);
        }
        None
    }

    async fn get_class_signature(
        session: &mut crate::session::DebugSession,
        class_id: u64,
    ) -> Option<String> {
        if let Some(signature) = session.class_signatures.get(&class_id) {
            return Some(signature.clone());
        }

        let signature = session.connection.get_signature(class_id).await.ok()?;
        session.class_signatures.insert(class_id, signature.clone());
        Some(signature)
    }

    async fn resolve_classes(
        session: &mut crate::session::DebugSession,
        class_pattern: &str,
    ) -> Result<Vec<ClassInfo>, String> {
        let mut matches = Vec::new();

        for signature in Self::candidate_signatures(class_pattern) {
            let mut exact = session
                .connection
                .classes_by_signature(&signature)
                .await
                .map_err(|e| format!("Failed to find class: {}", e))?;
            matches.append(&mut exact);
        }

        if !matches.is_empty() {
            matches.sort_by_key(|c| c.type_id);
            matches.dedup_by_key(|c| c.type_id);
            return Ok(matches);
        }

        let mut fallback: Vec<(u8, ClassInfo)> = session
            .connection
            .all_classes()
            .await
            .map_err(|e| format!("Failed to list loaded classes: {}", e))?
            .into_iter()
            .filter_map(|class_info| {
                Self::class_match_score(&class_info.signature, class_pattern)
                    .map(|score| (score, class_info))
            })
            .collect();

        fallback.sort_by_key(|(score, class_info)| (*score, class_info.type_id));
        let mut resolved = Vec::new();
        for (_, class_info) in fallback {
            if resolved
                .iter()
                .all(|existing: &ClassInfo| existing.type_id != class_info.type_id)
            {
                resolved.push(class_info);
            }
        }
        Ok(resolved)
    }

    pub fn new() -> Self {
        Self {
            session_manager: SessionManager::new(),
        }
    }

    pub async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params),
            "tools/list" => self.handle_list_tools(),
            "tools/call" => self.handle_call_tool(request.params).await,
            _ => Err(JsonRpcError {
                code: METHOD_NOT_FOUND,
                message: format!("Method not found: {}", request.method),
                data: None,
            }),
        };

        match result {
            Ok(value) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(value),
                error: None,
            },
            Err(error) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(error),
            },
        }
    }

    pub async fn handle_notification(&self, notification: JsonRpcNotification) {
        match notification.method.as_str() {
            "notifications/initialized" => {
                info!("Client initialized");
            }
            "notifications/cancelled" => {
                debug!("Request cancelled");
            }
            _ => {
                warn!("Unknown notification: {}", notification.method);
            }
        }
    }

    fn handle_initialize(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let _params: InitializeParams = serde_json::from_value(params.unwrap_or(json!({})))
            .map_err(|e| JsonRpcError {
                code: INVALID_PARAMS,
                message: format!("Invalid initialize params: {}", e),
                data: None,
            })?;

        let result = InitializeResult {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ServerCapabilities {
                tools: ToolsCapability {},
            },
            server_info: ServerInfo {
                name: "jdwp-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: Some(AGENT_INSTRUCTIONS.to_string()),
        };

        Ok(serde_json::to_value(result).unwrap())
    }

    fn handle_list_tools(&self) -> Result<serde_json::Value, JsonRpcError> {
        let result = ListToolsResult {
            tools: tools::get_tools(),
        };

        Ok(serde_json::to_value(result).unwrap())
    }

    async fn handle_call_tool(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let call_params: CallToolParams = serde_json::from_value(params.unwrap_or(json!({})))
            .map_err(|e| JsonRpcError {
                code: INVALID_PARAMS,
                message: format!("Invalid tool call params: {}", e),
                data: None,
            })?;

        // Route to appropriate handler based on tool name
        let result = match call_params.name.as_str() {
            "debug.attach" => self.handle_attach(call_params.arguments).await,
            "debug.set_breakpoint" => self.handle_set_breakpoint(call_params.arguments).await,
            "debug.list_breakpoints" => self.handle_list_breakpoints(call_params.arguments).await,
            "debug.clear_breakpoint" => self.handle_clear_breakpoint(call_params.arguments).await,
            "debug.continue" => self.handle_continue(call_params.arguments).await,
            "debug.step_into" => self.handle_step_into(call_params.arguments).await,
            "debug.step_over" => self.handle_step_over(call_params.arguments).await,
            "debug.step_out" => self.handle_step_out(call_params.arguments).await,
            "debug.get_stack" => self.handle_get_stack(call_params.arguments).await,
            "debug.get_variable" => self.handle_get_variable(call_params.arguments).await,
            "debug.select_thread" => self.handle_select_thread(call_params.arguments).await,
            "debug.list_threads" => self.handle_list_threads(call_params.arguments).await,
            "debug.pause" => self.handle_pause(call_params.arguments).await,
            "debug.disconnect" => self.handle_disconnect(call_params.arguments).await,
            "debug.get_last_event" => self.handle_get_last_event(call_params.arguments).await,
            "debug.wait_for_event" => self.handle_wait_for_event(call_params.arguments).await,
            "debug.inspect" => self.handle_inspect(call_params.arguments).await,
            "debug.find_class" => self.handle_find_class(call_params.arguments).await,
            "debug.list_methods" => self.handle_list_methods(call_params.arguments).await,
            "debug.exception_breakpoint" => {
                self.handle_exception_breakpoint(call_params.arguments)
                    .await
            }
            "debug.eval" => self.handle_eval(call_params.arguments).await,
            "debug.set_value" => self.handle_set_value(call_params.arguments).await,
            "debug.snapshot" => self.handle_snapshot(call_params.arguments).await,
            "debug.vm_info" => self.handle_vm_info(call_params.arguments).await,
            "debug.watch" => self.handle_watch(call_params.arguments).await,
            "debug.trace" => self.handle_trace(call_params.arguments).await,
            "debug.trace_result" => self.handle_trace_result(call_params.arguments).await,
            _ => Err(format!("Unknown tool: {}", call_params.name)),
        };

        match result {
            Ok(content) => {
                let call_result = CallToolResult {
                    content: vec![ContentBlock::Text { text: content }],
                    is_error: None,
                };
                Ok(serde_json::to_value(call_result).unwrap())
            }
            Err(error) => {
                let call_result = CallToolResult {
                    content: vec![ContentBlock::Text {
                        text: error.clone(),
                    }],
                    is_error: Some(true),
                };
                Ok(serde_json::to_value(call_result).unwrap())
            }
        }
    }

    // Tool implementations (stubs for now)
    async fn handle_attach(&self, args: serde_json::Value) -> Result<String, String> {
        let host = args
            .get("host")
            .and_then(|v| v.as_str())
            .unwrap_or("localhost");
        let port_u64 = args.get("port").and_then(|v| v.as_u64()).unwrap_or(5005);
        let port = u16::try_from(port_u64)
            .map_err(|_| format!("Port {} is out of valid range 1-65535", port_u64))?;
        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(5000);
        let allow_remote = args
            .get("allow_remote")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !allow_remote && !matches!(host, "localhost" | "127.0.0.1" | "::1") {
            return Err(format!(
                "Refusing remote JDWP attach to {}:{} without allow_remote=true",
                host, port
            ));
        }

        // Clean up any existing session before creating a new one
        if let Some(old_session_id) = self.session_manager.get_current_session_id().await {
            self.session_manager.remove_session(&old_session_id).await;
        }

        match jdwp_client::JdwpConnection::connect_with_timeout(host, port, timeout_ms).await {
            Ok(connection) => {
                // Create session
                let session_id = self.session_manager.create_session(connection).await;

                // Get session guard once to prevent race between spawn and store
                let session_guard = self
                    .session_manager
                    .get_current_session()
                    .await
                    .ok_or_else(|| "Failed to get session after creation".to_string())?;

                // Clone connection, spawn task, and store handle in single critical section
                {
                    let mut session = session_guard.lock().await;
                    let connection_clone = session.connection.clone();

                    // Spawn event listener task
                    let session_manager = self.session_manager.clone();
                    let task_handle = tokio::spawn(async move {
                        loop {
                            // Receive event without holding any locks!
                            let event_opt = connection_clone.recv_event().await;

                            // Store event (brief lock acquisition)
                            if let Some(event_set) = event_opt {
                                // Collect trace events (METHOD_ENTRY/EXIT) if tracing is active
                                let is_trace_event =
                                    Self::collect_trace_events(&session_manager, &event_set).await;

                                if is_trace_event {
                                    // If tracing with EventThread suspend, resume the JVM
                                    if event_set.suspend_policy > 0 {
                                        let _ = connection_clone.clone().resume_all().await;
                                    }
                                    continue;
                                }

                                let step_request_to_clear = {
                                    if let Some(session_guard) =
                                        session_manager.get_current_session().await
                                    {
                                        let session = session_guard.lock().await;
                                        event_set.events.iter().find_map(|event| {
                                            match &event.details {
                                                jdwp_client::events::EventKind::Step {
                                                    thread,
                                                    ..
                                                } => match &session.active_step {
                                                    Some(active)
                                                        if active.request_id
                                                            == event.request_id
                                                            && active.thread_id == *thread =>
                                                    {
                                                        Some(active.request_id)
                                                    }
                                                    _ => None,
                                                },
                                                _ => None,
                                            }
                                        })
                                    } else {
                                        None
                                    }
                                };

                                if let Some(request_id) = step_request_to_clear {
                                    if let Err(err) =
                                        connection_clone.clone().clear_step(request_id).await
                                    {
                                        warn!(
                                            "Failed to clear step request {} after event: {}",
                                            request_id, err
                                        );
                                    }
                                }

                                // Phase 1: Brief lock to clear step + extract condition data
                                let cond_info = if let Some(session_guard) =
                                    session_manager.get_current_session().await
                                {
                                    let mut session = session_guard.lock().await;
                                    if step_request_to_clear
                                        == session.active_step.as_ref().map(|s| s.request_id)
                                    {
                                        session.active_step = None;
                                    }

                                    // Extract condition data if this is a conditional breakpoint
                                    event_set.events.first().and_then(|e| {
                                        if let jdwp_client::events::EventKind::Breakpoint {
                                            thread,
                                            ..
                                        } = &e.details
                                        {
                                            let condition = session
                                                .breakpoints
                                                .values()
                                                .find(|bp| bp.request_id == e.request_id)
                                                .and_then(|bp| bp.condition.clone())?;
                                            Some((*thread, condition))
                                        } else {
                                            None
                                        }
                                    })
                                    // Lock released here
                                } else {
                                    break; // Session gone
                                };

                                // Phase 2: Evaluate condition WITHOUT holding lock (uses connection_clone)
                                let should_skip = if let Some((thread_id, condition)) = cond_info {
                                    let parts: Vec<&str> = condition.splitn(2, "==").collect();
                                    if parts.len() == 2 {
                                        let var_name = parts[0].trim().to_string();
                                        let expected = parts[1].trim().to_string();
                                        Self::eval_condition(
                                            &connection_clone,
                                            thread_id,
                                            &var_name,
                                            &expected,
                                        )
                                        .await
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                };

                                // Phase 3: Brief lock to store result or resume
                                if should_skip {
                                    let _ = connection_clone.clone().resume_all().await;
                                } else if let Some(session_guard) =
                                    session_manager.get_current_session().await
                                {
                                    let mut session = session_guard.lock().await;
                                    session.last_event = Some(event_set);
                                    session.last_event_seq += 1;
                                    session.last_event_notify.notify_waiters();
                                } else {
                                    break;
                                }
                            } else {
                                break; // Connection closed
                            }
                        }
                        info!("Event listener task stopped");
                    });

                    // Store task handle before releasing lock - prevents race with disconnect
                    session.event_listener_task = Some(task_handle);
                }

                Ok(format!(
                    "connected {}:{} session={}",
                    host, port, session_id
                ))
            }
            Err(e) => Err(format!("Failed to connect: {}", e)),
        }
    }

    async fn handle_set_breakpoint(&self, args: serde_json::Value) -> Result<String, String> {
        let class_pattern = args
            .get("class_pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'class_pattern' parameter".to_string())?;

        let line = args
            .get("line")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| "Missing 'line' parameter".to_string())? as i32;

        let method_hint = args.get("method").and_then(|v| v.as_str());

        // Get current session
        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session. Use debug.attach first.".to_string())?;

        let mut session = session_guard.lock().await;

        let classes = Self::resolve_classes(&mut session, class_pattern).await?;

        if classes.is_empty() {
            return Err(format!("Class not found: {}", class_pattern));
        }

        let mut chosen = None;

        for class in &classes {
            let methods = session
                .connection
                .get_methods(class.type_id)
                .await
                .map_err(|e| format!("Failed to get methods: {}", e))?;

            for method in &methods {
                if let Some(hint) = method_hint {
                    if method.name != hint {
                        continue;
                    }
                }

                if let Ok(line_table) = session
                    .connection
                    .get_line_table(class.type_id, method.method_id)
                    .await
                {
                    if let Some(line_entry) =
                        line_table.lines.iter().find(|e| e.line_number == line)
                    {
                        chosen = Some((class.clone(), method.clone(), line_entry.line_code_index));
                        break;
                    }
                }
            }

            if chosen.is_some() {
                break;
            }
        }

        let (class, method, line_code_index) = chosen.ok_or_else(|| {
            let resolved_names: Vec<String> = classes
                .iter()
                .map(|class| class.signature.clone())
                .collect();
            format!(
                "No method found containing line {} in class {}. Resolved classes: {}",
                line,
                class_pattern,
                resolved_names.join(", ")
            )
        })?;

        // Set the breakpoint!
        let request_id = session
            .connection
            .set_breakpoint(
                class.type_id,
                method.method_id,
                line_code_index,
                jdwp_client::SuspendPolicy::All,
            )
            .await
            .map_err(|e| format!("Failed to set breakpoint: {}", e))?;

        // Track the breakpoint in session
        let condition = args
            .get("condition")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(ref c) = condition {
            if c.splitn(2, "==").count() != 2 {
                return Err(format!(
                    "invalid condition format '{}', expected var_name==value",
                    c
                ));
            }
        }

        let bp_id = format!("bp_{}", request_id);
        let cond_desc = condition
            .as_deref()
            .map(|c| format!(" if {}", c))
            .unwrap_or_default();
        session.breakpoints.insert(
            bp_id.clone(),
            crate::session::BreakpointInfo {
                id: bp_id.clone(),
                request_id,
                class_pattern: class_pattern.to_string(),
                line: line as u32,
                method: Some(method.name.clone()),
                enabled: true,
                hit_count: 0,
                condition,
            },
        );

        Ok(format!(
            "bp {} at {}:{} class={} method={}{}",
            bp_id, class_pattern, line, class.signature, method.name, cond_desc
        ))
    }

    async fn handle_list_breakpoints(&self, _args: serde_json::Value) -> Result<String, String> {
        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;

        let session = session_guard.lock().await;

        if session.breakpoints.is_empty() && session.active_step.is_none() {
            return Ok("no breakpoints".to_string());
        }

        let mut output = format!("{} breakpoints:\n", session.breakpoints.len());

        for bp in session.breakpoints.values() {
            let method = bp.method.as_deref().unwrap_or("");
            let enabled = if bp.enabled { "+" } else { "-" };
            output.push_str(&format!(
                "{} {} {}:{} {}\n",
                enabled, bp.id, bp.class_pattern, bp.line, method
            ));
        }

        if let Some(s) = &session.active_step {
            output.push_str(&format!(
                "step {} thread=0x{:x} req={}\n",
                s.depth, s.thread_id, s.request_id
            ));
        }

        Ok(output)
    }

    async fn handle_clear_breakpoint(&self, args: serde_json::Value) -> Result<String, String> {
        let bp_id = args
            .get("breakpoint_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'breakpoint_id' parameter".to_string())?;

        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;

        let mut session = session_guard.lock().await;

        // Find the breakpoint
        let bp_info = session
            .breakpoints
            .get(bp_id)
            .ok_or_else(|| format!("Breakpoint not found: {}", bp_id))?
            .clone();

        // Clear the breakpoint in the JVM
        session
            .connection
            .clear_breakpoint(bp_info.request_id)
            .await
            .map_err(|e| format!("Failed to clear breakpoint: {}", e))?;

        // Remove from session
        session.breakpoints.remove(bp_id);

        Ok(format!(
            "cleared {} {}:{}",
            bp_id, bp_info.class_pattern, bp_info.line
        ))
    }

    async fn handle_continue(&self, _args: serde_json::Value) -> Result<String, String> {
        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;

        let mut session = session_guard.lock().await;

        session
            .connection
            .resume_all()
            .await
            .map_err(|e| format!("Failed to resume: {}", e))?;

        Ok("resumed".to_string())
    }

    async fn handle_step_into(&self, args: serde_json::Value) -> Result<String, String> {
        self.handle_step(args, jdwp_client::StepDepth::Into, "into")
            .await
    }

    async fn handle_step_over(&self, args: serde_json::Value) -> Result<String, String> {
        self.handle_step(args, jdwp_client::StepDepth::Over, "over")
            .await
    }

    async fn handle_step_out(&self, args: serde_json::Value) -> Result<String, String> {
        self.handle_step(args, jdwp_client::StepDepth::Out, "out")
            .await
    }

    fn parse_thread_id(args: &serde_json::Value) -> Option<u64> {
        args.get("thread_id")
            .and_then(|v| v.as_str())
            .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
    }

    fn event_thread_id(event_set: &jdwp_client::EventSet) -> Option<u64> {
        let event = event_set.events.first()?;
        match &event.details {
            jdwp_client::events::EventKind::Breakpoint { thread, .. }
            | jdwp_client::events::EventKind::Step { thread, .. }
            | jdwp_client::events::EventKind::VMStart { thread }
            | jdwp_client::events::EventKind::ThreadStart { thread }
            | jdwp_client::events::EventKind::ThreadDeath { thread }
            | jdwp_client::events::EventKind::ClassPrepare { thread, .. }
            | jdwp_client::events::EventKind::Exception { thread, .. }
            | jdwp_client::events::EventKind::MethodEntry { thread, .. }
            | jdwp_client::events::EventKind::MethodExit { thread, .. }
            | jdwp_client::events::EventKind::FieldAccess { thread, .. }
            | jdwp_client::events::EventKind::FieldModification { thread, .. } => Some(*thread),
            _ => None,
        }
    }

    async fn resolve_target_thread(
        &self,
        session: &mut crate::session::DebugSession,
        thread_id: Option<u64>,
    ) -> Result<u64, String> {
        if let Some(tid) = thread_id {
            return Ok(tid);
        }
        if let Some(tid) = session.selected_thread_id {
            return Ok(tid);
        }
        if let Some(event_set) = &session.last_event {
            if let Some(tid) = Self::event_thread_id(event_set) {
                return Ok(tid);
            }
        }
        let threads = session
            .connection
            .get_all_threads()
            .await
            .map_err(|e| format!("Failed to get threads: {}", e))?;
        threads
            .first()
            .copied()
            .ok_or_else(|| "No threads found".to_string())
    }

    async fn handle_step(
        &self,
        args: serde_json::Value,
        depth: jdwp_client::StepDepth,
        depth_label: &str,
    ) -> Result<String, String> {
        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;

        let mut session = session_guard.lock().await;
        let thread_id = Self::parse_thread_id(&args);
        let target_thread = self.resolve_target_thread(&mut session, thread_id).await?;

        if let Some(active_step) = session.active_step.take() {
            if let Err(err) = session.connection.clear_step(active_step.request_id).await {
                warn!(
                    "Failed to clear previous step request {} on thread 0x{:x}: {}",
                    active_step.request_id, active_step.thread_id, err
                );
            }
        }

        let request_id = session
            .connection
            .set_step(
                target_thread,
                jdwp_client::StepSize::Line,
                depth,
                jdwp_client::SuspendPolicy::All,
            )
            .await
            .map_err(|e| format!("Failed to set step request: {}", e))?;

        session.active_step = Some(StepRequestInfo {
            request_id,
            thread_id: target_thread,
            depth: depth_label.to_string(),
        });

        session
            .connection
            .resume_all()
            .await
            .map_err(|e| format!("Failed to resume after setting step request: {}", e))?;

        Ok(format!(
            "step {} thread=0x{:x} req={} resumed",
            depth_label, target_thread, request_id
        ))
    }

    /// Compact format — resolve strings, objects as @hex
    async fn format_value_leaf(
        session: &mut crate::session::DebugSession,
        value: &jdwp_client::types::Value,
    ) -> String {
        const MAX_STR: usize = 80;
        if value.tag == 115 {
            if let jdwp_client::types::ValueData::Object(oid) = &value.data {
                if *oid != 0 {
                    return match session.connection.get_string_value(*oid).await {
                        Ok(s) if s.len() > MAX_STR => format!("\"{}...\"", &s[..MAX_STR]),
                        Ok(s) => format!("\"{}\"", s),
                        Err(_) => format!("str@{:x}", oid),
                    };
                }
                return "null".to_string();
            }
        }
        value.format_compact()
    }

    /// Resolve object one level deep: ClassName{field1=val, field2=val, ...}
    /// Falls back to leaf format for primitives/strings/errors.
    async fn format_value_resolved(
        session: &mut crate::session::DebugSession,
        value: &jdwp_client::types::Value,
    ) -> String {
        const MAX_FIELDS: usize = 8;

        // Non-object or null → leaf
        let object_id = match &value.data {
            jdwp_client::types::ValueData::Object(oid) if *oid != 0 && value.tag != 115 => *oid,
            _ => return Self::format_value_leaf(session, value).await,
        };

        // Array → just tag
        if value.tag == 91 {
            return format!("array@{:x}", object_id);
        }

        // Resolve class
        let ref_type_id = match session
            .connection
            .get_object_reference_type(object_id)
            .await
        {
            Ok(id) => id,
            Err(_) => return format!("@{:x}", object_id),
        };

        let class_name = match Self::get_class_signature(session, ref_type_id).await {
            Some(sig) => Self::signature_to_display_name(&sig).unwrap_or(sig),
            None => "?".to_string(),
        };

        // Get instance fields (skip static, synthetic)
        let fields = match session.connection.get_fields(ref_type_id).await {
            Ok(f) => f,
            Err(_) => return format!("{}@{:x}", class_name, object_id),
        };

        let instance_fields: Vec<_> = fields
            .iter()
            .filter(|f| {
                f.mod_bits & 0x0008 == 0 // not static
                    && !f.name.starts_with('$')
            })
            .collect();

        if instance_fields.is_empty() {
            return format!("{}@{:x}", class_name, object_id);
        }

        let field_ids: Vec<_> = instance_fields
            .iter()
            .take(MAX_FIELDS)
            .map(|f| f.field_id)
            .collect();
        let field_values = match session
            .connection
            .get_object_values(object_id, field_ids)
            .await
        {
            Ok(v) => v,
            Err(_) => return format!("{}@{:x}", class_name, object_id),
        };

        let mut pairs = Vec::new();
        for (field, val) in instance_fields
            .iter()
            .take(MAX_FIELDS)
            .zip(field_values.iter())
        {
            let fval = Self::format_value_leaf(session, val).await;
            pairs.push(format!("{}={}", field.name, fval));
        }

        let extra = if instance_fields.len() > MAX_FIELDS {
            format!(", ...+{}", instance_fields.len() - MAX_FIELDS)
        } else {
            String::new()
        };

        format!("{}{{{}{}}}", class_name, pairs.join(", "), extra)
    }

    async fn handle_get_stack(&self, args: serde_json::Value) -> Result<String, String> {
        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;

        let mut session = session_guard.lock().await;

        let thread_id = Self::parse_thread_id(&args);

        let max_frames = args
            .get("max_frames")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .min(200) as usize;

        let include_variables = args
            .get("include_variables")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let resolve_objects = args
            .get("max_variable_depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(2)
            >= 2;

        let target_thread = self.resolve_target_thread(&mut session, thread_id).await?;

        // Get frames (-1 means all frames to avoid INVALID_LENGTH errors)
        let mut frames = session
            .connection
            .get_frames(target_thread, 0, -1)
            .await
            .map_err(|e| format!("Failed to get frames: {}", e))?;

        // Truncate to max_frames
        frames.truncate(max_frames);

        if frames.is_empty() {
            return Ok(format!("thread 0x{:x}: no frames", target_thread));
        }

        let mut output = format!("thread 0x{:x}, {} frames:\n", target_thread, frames.len());

        for (idx, frame) in frames.iter().enumerate() {
            // Resolve class and method names for compact display
            let class_display = if let Some(sig) =
                Self::get_class_signature(&mut session, frame.location.class_id).await
            {
                Self::signature_to_display_name(&sig).unwrap_or(sig)
            } else {
                format!("0x{:x}", frame.location.class_id)
            };

            let mut method_name = None;
            let mut var_line = String::new();

            if let Ok(methods) = session
                .connection
                .get_methods(frame.location.class_id)
                .await
            {
                if let Some(method) = methods
                    .iter()
                    .find(|m| m.method_id == frame.location.method_id)
                {
                    method_name = Some(method.name.clone());

                    if include_variables {
                        if let Ok(var_table) = session
                            .connection
                            .get_variable_table(frame.location.class_id, frame.location.method_id)
                            .await
                        {
                            let current_index = frame.location.index;
                            let active_vars: Vec<_> = var_table
                                .iter()
                                .filter(|v| {
                                    current_index >= v.code_index
                                        && current_index < v.code_index + v.length as u64
                                })
                                .collect();

                            if !active_vars.is_empty() {
                                let slots: Vec<jdwp_client::stackframe::VariableSlot> = active_vars
                                    .iter()
                                    .map(|v| jdwp_client::stackframe::VariableSlot {
                                        slot: v.slot as i32,
                                        sig_byte: v
                                            .signature
                                            .as_bytes()
                                            .first()
                                            .copied()
                                            .unwrap_or(b'L'),
                                    })
                                    .collect();

                                if let Ok(values) = session
                                    .connection
                                    .get_frame_values(target_thread, frame.frame_id, slots)
                                    .await
                                {
                                    let mut pairs = Vec::new();
                                    for (var, val) in active_vars.iter().zip(values.iter()) {
                                        let formatted = if resolve_objects {
                                            Self::format_value_resolved(&mut session, val).await
                                        } else {
                                            Self::format_value_leaf(&mut session, val).await
                                        };
                                        pairs.push(format!("{}={}", var.name, formatted));
                                    }
                                    var_line = format!("  {}\n", pairs.join("  "));
                                }
                            }
                        }
                    }
                }
            }

            let mname = method_name.as_deref().unwrap_or("?");
            output.push_str(&format!(
                "#{} {}.{}:{}\n",
                idx, class_display, mname, frame.location.index
            ));
            if !var_line.is_empty() {
                output.push_str(&var_line);
            }
        }

        Ok(output)
    }

    async fn handle_get_variable(&self, args: serde_json::Value) -> Result<String, String> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'name' parameter".to_string())?;
        let frame_index = args
            .get("frame_index")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        if frame_index < 0 {
            return Err("frame_index must be >= 0".to_string());
        }

        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;
        let mut session = session_guard.lock().await;

        let thread_id = Self::parse_thread_id(&args);
        let target_thread = self.resolve_target_thread(&mut session, thread_id).await?;

        let frames = session
            .connection
            .get_frames(target_thread, 0, frame_index as i32 + 1)
            .await
            .map_err(|e| format!("Failed to get frames: {}", e))?;
        let frame = frames
            .get(frame_index as usize)
            .ok_or_else(|| format!("Frame {} not available", frame_index))?;

        let var_table = session
            .connection
            .get_variable_table(frame.location.class_id, frame.location.method_id)
            .await
            .map_err(|e| format!("Failed to get variable table: {}", e))?;
        let current_index = frame.location.index;
        let active_vars: Vec<_> = var_table
            .iter()
            .filter(|v| {
                current_index >= v.code_index && current_index < v.code_index + v.length as u64
            })
            .collect();

        let var = active_vars
            .iter()
            .find(|v| v.name == name)
            .ok_or_else(|| format!("Variable '{}' not found in frame {}", name, frame_index))?;

        let slots = vec![jdwp_client::stackframe::VariableSlot {
            slot: var.slot as i32,
            sig_byte: var.signature.as_bytes().first().copied().unwrap_or(b'L'),
        }];
        let values = session
            .connection
            .get_frame_values(target_thread, frame.frame_id, slots)
            .await
            .map_err(|e| format!("Failed to get frame value: {}", e))?;
        let value = values
            .first()
            .ok_or_else(|| "JDWP returned no variable value".to_string())?;
        let formatted_value = Self::format_value_resolved(&mut session, value).await;

        Ok(format!("{}={} ({})", name, formatted_value, var.signature))
    }

    async fn handle_select_thread(&self, args: serde_json::Value) -> Result<String, String> {
        let thread_id = Self::parse_thread_id(&args)
            .ok_or_else(|| "Missing or invalid 'thread_id' parameter".to_string())?;

        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;
        let mut session = session_guard.lock().await;

        let threads = session
            .connection
            .get_all_threads()
            .await
            .map_err(|e| format!("Failed to get threads: {}", e))?;
        if !threads.contains(&thread_id) {
            return Err(format!("Thread 0x{:x} not found in JVM", thread_id));
        }

        session.selected_thread_id = Some(thread_id);
        Ok(format!("selected thread 0x{:x}", thread_id))
    }

    async fn handle_list_threads(&self, _args: serde_json::Value) -> Result<String, String> {
        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;

        let mut session = session_guard.lock().await;

        let threads = session
            .connection
            .get_all_threads()
            .await
            .map_err(|e| format!("Failed to get threads: {}", e))?;
        let selected_thread_id = session.selected_thread_id;
        let event_thread_id = session.last_event.as_ref().and_then(Self::event_thread_id);

        let mut output = format!("{} threads:\n", threads.len());

        for thread_id in threads.iter() {
            let mut flags = String::new();
            if selected_thread_id == Some(*thread_id) {
                flags.push_str(" *selected");
            }
            if event_thread_id == Some(*thread_id) {
                flags.push_str(" *event");
            }

            let status = match session.connection.get_frames(*thread_id, 0, 1).await {
                Ok(frames) if !frames.is_empty() => "suspended",
                Ok(_) => "running",
                Err(_) => "?",
            };

            output.push_str(&format!("0x{:x} {}{}\n", thread_id, status, flags));
        }

        Ok(output)
    }

    async fn handle_pause(&self, _args: serde_json::Value) -> Result<String, String> {
        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;

        let mut session = session_guard.lock().await;

        session
            .connection
            .suspend_all()
            .await
            .map_err(|e| format!("Failed to suspend: {}", e))?;

        Ok("paused".to_string())
    }

    async fn handle_disconnect(&self, _args: serde_json::Value) -> Result<String, String> {
        let current_session_id = self.session_manager.get_current_session_id().await;

        if let Some(session_id) = current_session_id {
            // Remove the session (this will also clear current session)
            self.session_manager.remove_session(&session_id).await;
            Ok(format!("disconnected {}", session_id))
        } else {
            Err("No active debug session to disconnect".to_string())
        }
    }

    async fn handle_get_last_event(&self, _args: serde_json::Value) -> Result<String, String> {
        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;

        let session = session_guard.lock().await;

        if let Some(event_set) = &session.last_event {
            let mut output = String::new();

            for event in event_set.events.iter() {
                let line = match &event.details {
                    jdwp_client::events::EventKind::Breakpoint { thread, location } => {
                        format!(
                            "breakpoint thread=0x{:x} class=0x{:x} method=0x{:x} idx={}",
                            thread, location.class_id, location.method_id, location.index
                        )
                    }
                    jdwp_client::events::EventKind::Step { thread, location } => {
                        format!(
                            "step thread=0x{:x} class=0x{:x} method=0x{:x} idx={}",
                            thread, location.class_id, location.method_id, location.index
                        )
                    }
                    jdwp_client::events::EventKind::VMStart { thread } => {
                        format!("vm_start thread=0x{:x}", thread)
                    }
                    jdwp_client::events::EventKind::VMDeath => "vm_death".to_string(),
                    jdwp_client::events::EventKind::ThreadStart { thread } => {
                        format!("thread_start 0x{:x}", thread)
                    }
                    jdwp_client::events::EventKind::ThreadDeath { thread } => {
                        format!("thread_death 0x{:x}", thread)
                    }
                    jdwp_client::events::EventKind::ClassPrepare {
                        thread, signature, ..
                    } => {
                        format!("class_prepare thread=0x{:x} {}", thread, signature)
                    }
                    jdwp_client::events::EventKind::Exception {
                        thread, exception, ..
                    } => {
                        format!("exception thread=0x{:x} obj=0x{:x}", thread, exception)
                    }
                    jdwp_client::events::EventKind::MethodEntry { thread, location } => {
                        format!(
                            "method_entry thread=0x{:x} method=0x{:x}",
                            thread, location.method_id
                        )
                    }
                    jdwp_client::events::EventKind::MethodExit { thread, location } => {
                        format!(
                            "method_exit thread=0x{:x} method=0x{:x}",
                            thread, location.method_id
                        )
                    }
                    jdwp_client::events::EventKind::FieldModification {
                        thread,
                        field_id,
                        object_id,
                        new_value,
                        ..
                    } => {
                        format!(
                            "field_modified thread=0x{:x} field=0x{:x} obj=0x{:x} new={}",
                            thread, field_id, object_id, new_value
                        )
                    }
                    jdwp_client::events::EventKind::FieldAccess {
                        thread,
                        field_id,
                        object_id,
                        ..
                    } => {
                        format!(
                            "field_access thread=0x{:x} field=0x{:x} obj=0x{:x}",
                            thread, field_id, object_id
                        )
                    }
                    _ => format!("unknown kind={}", event.kind),
                };
                output.push_str(&line);
                output.push('\n');
            }

            Ok(output)
        } else {
            Ok("no events yet".to_string())
        }
    }

    async fn handle_wait_for_event(&self, args: serde_json::Value) -> Result<String, String> {
        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(30000)
            .min(120_000); // Cap at 2 minutes to prevent blocking the server
        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;

        let (start_seq, notify) = {
            let session = session_guard.lock().await;
            (session.last_event_seq, session.last_event_notify.clone())
        };

        let wait_result =
            tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), async {
                loop {
                    // Subscribe BEFORE checking — prevents missed notifications
                    let future = notify.notified();
                    tokio::pin!(future);
                    future.as_mut().enable();

                    // Check if event already arrived
                    {
                        let session = session_guard.lock().await;
                        if session.last_event_seq > start_seq {
                            return session.last_event.clone();
                        }
                    }

                    // Wait for notification (subscription was registered before check)
                    future.await;
                }
            })
            .await;

        match wait_result {
            Ok(Some(event_set)) => {
                let thread_text = Self::event_thread_id(&event_set)
                    .map(|tid| format!(" thread=0x{:x}", tid))
                    .unwrap_or_default();
                Ok(format!(
                    "event: count={} suspend={}{}",
                    event_set.events.len(),
                    event_set.suspend_policy,
                    thread_text
                ))
            }
            Ok(None) => Err("no event captured".to_string()),
            Err(_) => Err(format!("timeout after {}ms", timeout_ms)),
        }
    }

    async fn handle_inspect(&self, args: serde_json::Value) -> Result<String, String> {
        let object_id_str = args
            .get("object_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'object_id' parameter".to_string())?;
        let object_id = u64::from_str_radix(object_id_str.trim_start_matches("0x"), 16)
            .map_err(|_| format!("Invalid object_id: {}", object_id_str))?;

        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;
        let mut session = session_guard.lock().await;

        // Get class
        let ref_type_id = session
            .connection
            .get_object_reference_type(object_id)
            .await
            .map_err(|e| format!("Failed to get object type: {}", e))?;

        let class_name = match Self::get_class_signature(&mut session, ref_type_id).await {
            Some(sig) => Self::signature_to_display_name(&sig).unwrap_or(sig),
            None => "?".to_string(),
        };

        let fields = session
            .connection
            .get_fields(ref_type_id)
            .await
            .map_err(|e| format!("Failed to get fields: {}", e))?;

        if fields.is_empty() {
            return Ok(format!("{} (no fields)", class_name));
        }

        let field_ids: Vec<_> = fields.iter().map(|f| f.field_id).collect();
        let values = session
            .connection
            .get_object_values(object_id, field_ids)
            .await
            .map_err(|e| format!("Failed to get field values: {}", e))?;

        let mut output = format!("{} {{\n", class_name);
        for (field, val) in fields.iter().zip(values.iter()) {
            let is_static = field.mod_bits & 0x0008 != 0;
            let prefix = if is_static { "static " } else { "" };
            let fval = Self::format_value_resolved(&mut session, val).await;
            output.push_str(&format!(
                "  {}{}: {} = {}\n",
                prefix, field.name, field.signature, fval
            ));
        }
        output.push('}');

        Ok(output)
    }

    async fn handle_find_class(&self, args: serde_json::Value) -> Result<String, String> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'pattern' parameter".to_string())?;

        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;
        let mut session = session_guard.lock().await;

        let classes = Self::resolve_classes(&mut session, pattern).await?;

        if classes.is_empty() {
            return Ok(format!("no classes matching '{}'", pattern));
        }

        let mut output = String::new();
        for class in &classes {
            let display = Self::signature_to_display_name(&class.signature)
                .unwrap_or_else(|| class.signature.clone());
            output.push_str(&display);
            output.push('\n');
        }
        Ok(output)
    }

    async fn handle_list_methods(&self, args: serde_json::Value) -> Result<String, String> {
        let class_pattern = args
            .get("class_pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'class_pattern' parameter".to_string())?;

        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;
        let mut session = session_guard.lock().await;

        let classes = Self::resolve_classes(&mut session, class_pattern).await?;

        if classes.is_empty() {
            return Err(format!("class not found: {}", class_pattern));
        }

        let class = &classes[0];
        let class_display = Self::signature_to_display_name(&class.signature)
            .unwrap_or_else(|| class.signature.clone());

        let methods = session
            .connection
            .get_methods(class.type_id)
            .await
            .map_err(|e| format!("Failed to get methods: {}", e))?;

        let mut output = format!("{}:\n", class_display);

        for method in &methods {
            // Skip synthetic/bridge methods
            if method.name.contains('$') || method.mod_bits & 0x1040 != 0 {
                continue;
            }

            // Get line range
            let line_range = match session
                .connection
                .get_line_table(class.type_id, method.method_id)
                .await
            {
                Ok(lt) if !lt.lines.is_empty() => {
                    let first = lt.lines.first().unwrap().line_number;
                    let last = lt.lines.last().unwrap().line_number;
                    format!(":{}-{}", first, last)
                }
                _ => String::new(),
            };

            output.push_str(&format!("  {}{}\n", method.name, line_range));
        }

        Ok(output)
    }

    async fn handle_exception_breakpoint(&self, args: serde_json::Value) -> Result<String, String> {
        let caught = args.get("caught").and_then(|v| v.as_bool()).unwrap_or(true);
        let uncaught = args
            .get("uncaught")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let class_pattern = args.get("class_pattern").and_then(|v| v.as_str());

        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;
        let mut session = session_guard.lock().await;

        // Resolve exception class if pattern given, otherwise 0 = all exceptions
        let exception_class_id = if let Some(pattern) = class_pattern {
            let classes = Self::resolve_classes(&mut session, pattern).await?;
            classes
                .first()
                .map(|c| c.type_id)
                .ok_or_else(|| format!("class not found: {}", pattern))?
        } else {
            0
        };

        let request_id = session
            .connection
            .set_exception_breakpoint(
                exception_class_id,
                caught,
                uncaught,
                jdwp_client::SuspendPolicy::All,
            )
            .await
            .map_err(|e| format!("Failed to set exception breakpoint: {}", e))?;

        let class_desc = class_pattern.unwrap_or("*");
        let scope = match (caught, uncaught) {
            (true, true) => "caught+uncaught",
            (true, false) => "caught",
            (false, true) => "uncaught",
            (false, false) => "none",
        };
        Ok(format!(
            "exception bp req={} class={} scope={}",
            request_id, class_desc, scope
        ))
    }

    async fn handle_eval(&self, args: serde_json::Value) -> Result<String, String> {
        let object_id_str = args
            .get("object_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'object_id'".to_string())?;
        let object_id = u64::from_str_radix(object_id_str.trim_start_matches("0x"), 16)
            .map_err(|_| format!("Invalid object_id: {}", object_id_str))?;
        let method_name = args
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("toString");

        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;
        let mut session = session_guard.lock().await;

        // Need a suspended thread for invocation
        let thread_id = Self::parse_thread_id(&args);
        let target_thread = self.resolve_target_thread(&mut session, thread_id).await?;

        // Verify thread is suspended (required for method invocation)
        match session.connection.get_frames(target_thread, 0, 1).await {
            Ok(f) if !f.is_empty() => {} // has frames = suspended
            _ => {
                return Err(format!(
                    "thread 0x{:x} is not suspended. Hit a breakpoint or use debug.pause first",
                    target_thread
                ));
            }
        }

        // Get object's class
        let ref_type_id = session
            .connection
            .get_object_reference_type(object_id)
            .await
            .map_err(|e| format!("Failed to get object type: {}", e))?;

        // Find the method
        let methods = session
            .connection
            .get_methods(ref_type_id)
            .await
            .map_err(|e| format!("Failed to get methods: {}", e))?;

        let method = methods
            .iter()
            .find(|m| m.name == method_name && m.signature.starts_with("()"))
            .ok_or_else(|| format!("no zero-arg method '{}' found", method_name))?;

        let method_id = method.method_id;

        // Invoke with no args, single-threaded
        let (return_value, exception_id) = session
            .connection
            .invoke_method(object_id, target_thread, ref_type_id, method_id, &[], true)
            .await
            .map_err(|e| format!("Invoke failed: {}", e))?;

        if exception_id != 0 {
            // Get exception class name + invoke toString()
            let exc_class = match session
                .connection
                .get_object_reference_type(exception_id)
                .await
            {
                Ok(rt) => Self::get_class_signature(&mut session, rt)
                    .await
                    .and_then(|s| Self::signature_to_display_name(&s))
                    .unwrap_or_else(|| "?".to_string()),
                Err(_) => "?".to_string(),
            };
            // Try toString() on the exception for the message
            let exc_msg = match session
                .connection
                .get_object_reference_type(exception_id)
                .await
            {
                Ok(rt) => {
                    let methods = session.connection.get_methods(rt).await.unwrap_or_default();
                    if let Some(to_string) = methods
                        .iter()
                        .find(|m| m.name == "toString" && m.signature.starts_with("()"))
                    {
                        match session
                            .connection
                            .invoke_method(
                                exception_id,
                                target_thread,
                                rt,
                                to_string.method_id,
                                &[],
                                true,
                            )
                            .await
                        {
                            Ok((val, 0)) => Self::format_value_leaf(&mut session, &val).await,
                            _ => exc_class.clone(),
                        }
                    } else {
                        exc_class.clone()
                    }
                }
                Err(_) => exc_class.clone(),
            };
            return Err(format!("threw {}: {}", exc_class, exc_msg));
        }

        let formatted = Self::format_value_resolved(&mut session, &return_value).await;
        Ok(formatted)
    }

    async fn handle_set_value(&self, args: serde_json::Value) -> Result<String, String> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'name'".to_string())?;
        let frame_index = args
            .get("frame_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;
        let mut session = session_guard.lock().await;

        let thread_id = Self::parse_thread_id(&args);
        let target_thread = self.resolve_target_thread(&mut session, thread_id).await?;

        let frames = session
            .connection
            .get_frames(target_thread, 0, frame_index as i32 + 1)
            .await
            .map_err(|e| format!("Failed to get frames: {}", e))?;
        let frame = frames
            .get(frame_index)
            .ok_or_else(|| format!("Frame {} not available", frame_index))?;

        let var_table = session
            .connection
            .get_variable_table(frame.location.class_id, frame.location.method_id)
            .await
            .map_err(|e| format!("Failed to get variable table: {}", e))?;
        let current_index = frame.location.index;
        let var = var_table
            .iter()
            .filter(|v| {
                current_index >= v.code_index && current_index < v.code_index + v.length as u64
            })
            .find(|v| v.name == name)
            .ok_or_else(|| format!("Variable '{}' not found", name))?;

        let slot = var.slot as i32;
        let sig_byte = var.signature.as_bytes().first().copied().unwrap_or(b'L');

        // Build the value to set based on signature type and the "value" arg
        let new_value = Self::parse_set_value(&args, sig_byte, &mut session).await?;

        let frame_id = frame.frame_id;
        session
            .connection
            .set_frame_values(target_thread, frame_id, vec![(slot, new_value.clone())])
            .await
            .map_err(|e| format!("Failed to set value: {}", e))?;

        let formatted = Self::format_value_leaf(&mut session, &new_value).await;
        Ok(format!("{}={}", name, formatted))
    }

    async fn parse_set_value(
        args: &serde_json::Value,
        sig_byte: u8,
        session: &mut crate::session::DebugSession,
    ) -> Result<jdwp_client::types::Value, String> {
        use jdwp_client::types::{Value, ValueData};
        let raw = args
            .get("value")
            .ok_or_else(|| "Missing 'value' parameter".to_string())?;

        let (tag, data) = match sig_byte {
            b'I' => {
                let v = raw.as_i64().ok_or("value must be integer")? as i32;
                (b'I', ValueData::Int(v))
            }
            b'J' => {
                let v = raw.as_i64().ok_or("value must be integer")?;
                (b'J', ValueData::Long(v))
            }
            b'S' => {
                let v = raw.as_i64().ok_or("value must be integer")? as i16;
                (b'S', ValueData::Short(v))
            }
            b'B' => {
                let v = raw.as_i64().ok_or("value must be integer")? as i8;
                (b'B', ValueData::Byte(v))
            }
            b'Z' => {
                let v = raw.as_bool().ok_or("value must be boolean")?;
                (b'Z', ValueData::Boolean(v))
            }
            b'F' => {
                let v = raw.as_f64().ok_or("value must be number")? as f32;
                (b'F', ValueData::Float(v))
            }
            b'D' => {
                let v = raw.as_f64().ok_or("value must be number")?;
                (b'D', ValueData::Double(v))
            }
            b'C' => {
                let s = raw.as_str().ok_or("value must be string for char")?;
                let c = s.chars().next().ok_or("empty string for char")?;
                (b'C', ValueData::Char(c as u16))
            }
            // String or object reference
            b'L' | b's' => {
                if let Some(s) = raw.as_str() {
                    // Create a string in the JVM
                    let string_id = session
                        .connection
                        .create_string(s)
                        .await
                        .map_err(|e| format!("Failed to create string: {}", e))?;
                    (115u8, ValueData::Object(string_id)) // 's' = string tag
                } else if raw.is_null() {
                    (b'L', ValueData::Object(0))
                } else {
                    return Err("value must be string or null for object type".to_string());
                }
            }
            _ => return Err(format!("unsupported variable type: {}", sig_byte as char)),
        };

        Ok(Value { tag, data })
    }

    async fn handle_snapshot(&self, _args: serde_json::Value) -> Result<String, String> {
        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;
        let mut session = session_guard.lock().await;

        let mut output = String::new();

        // 1. Last event
        if let Some(event_set) = &session.last_event {
            for event in &event_set.events {
                let desc = match &event.details {
                    jdwp_client::events::EventKind::Breakpoint { thread, location } => {
                        format!(
                            "breakpoint thread=0x{:x} class=0x{:x} idx={}",
                            thread, location.class_id, location.index
                        )
                    }
                    jdwp_client::events::EventKind::Step { thread, location } => {
                        format!(
                            "step thread=0x{:x} class=0x{:x} idx={}",
                            thread, location.class_id, location.index
                        )
                    }
                    jdwp_client::events::EventKind::Exception {
                        thread, exception, ..
                    } => {
                        format!("exception thread=0x{:x} obj=0x{:x}", thread, exception)
                    }
                    _ => format!("event kind={}", event.kind),
                };
                output.push_str(&format!("[event] {}\n", desc));
            }
        } else {
            output.push_str("[event] none\n");
        }

        // 2. Active breakpoints
        if !session.breakpoints.is_empty() {
            for bp in session.breakpoints.values() {
                output.push_str(&format!(
                    "[bp] {} {}:{}\n",
                    bp.id, bp.class_pattern, bp.line
                ));
            }
        }

        // 3. Stack of event thread (if available)
        let thread_id = session
            .last_event
            .as_ref()
            .and_then(Self::event_thread_id)
            .or(session.selected_thread_id);

        if let Some(tid) = thread_id {
            if let Ok(frames) = session.connection.get_frames(tid, 0, -1).await {
                let max_frames = 10;
                output.push_str(&format!(
                    "[stack] thread=0x{:x}, {} frames:\n",
                    tid,
                    frames.len()
                ));
                for (idx, frame) in frames.iter().take(max_frames).enumerate() {
                    let class_display = if let Some(sig) =
                        Self::get_class_signature(&mut session, frame.location.class_id).await
                    {
                        Self::signature_to_display_name(&sig).unwrap_or(sig)
                    } else {
                        format!("0x{:x}", frame.location.class_id)
                    };

                    let mname = if let Ok(methods) = session
                        .connection
                        .get_methods(frame.location.class_id)
                        .await
                    {
                        methods
                            .iter()
                            .find(|m| m.method_id == frame.location.method_id)
                            .map(|m| m.name.clone())
                            .unwrap_or_else(|| "?".to_string())
                    } else {
                        "?".to_string()
                    };

                    output.push_str(&format!(
                        "  #{} {}.{}:{}\n",
                        idx, class_display, mname, frame.location.index
                    ));

                    // Variables for top 3 frames only
                    if idx < 3 {
                        if let Ok(var_table) = session
                            .connection
                            .get_variable_table(frame.location.class_id, frame.location.method_id)
                            .await
                        {
                            let ci = frame.location.index;
                            let active: Vec<_> = var_table
                                .iter()
                                .filter(|v| {
                                    ci >= v.code_index && ci < v.code_index + v.length as u64
                                })
                                .collect();

                            if !active.is_empty() {
                                let slots: Vec<_> = active
                                    .iter()
                                    .map(|v| jdwp_client::stackframe::VariableSlot {
                                        slot: v.slot as i32,
                                        sig_byte: v
                                            .signature
                                            .as_bytes()
                                            .first()
                                            .copied()
                                            .unwrap_or(b'L'),
                                    })
                                    .collect();

                                if let Ok(values) = session
                                    .connection
                                    .get_frame_values(tid, frame.frame_id, slots)
                                    .await
                                {
                                    let mut pairs = Vec::new();
                                    for (var, val) in active.iter().zip(values.iter()) {
                                        let f =
                                            Self::format_value_resolved(&mut session, val).await;
                                        pairs.push(format!("{}={}", var.name, f));
                                    }
                                    output.push_str(&format!("    {}\n", pairs.join("  ")));
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(output)
    }

    async fn handle_vm_info(&self, _args: serde_json::Value) -> Result<String, String> {
        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;
        let mut session = session_guard.lock().await;

        let version = session
            .connection
            .get_version()
            .await
            .map_err(|e| format!("Failed to get version: {}", e))?;

        Ok(format!(
            "{} {} (JDWP {}.{})",
            version.vm_name, version.vm_version, version.jdwp_major, version.jdwp_minor
        ))
    }

    async fn handle_watch(&self, args: serde_json::Value) -> Result<String, String> {
        let class_pattern = args
            .get("class_pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'class_pattern'".to_string())?;
        let field_name = args
            .get("field")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'field'".to_string())?;

        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;
        let mut session = session_guard.lock().await;

        let classes = Self::resolve_classes(&mut session, class_pattern).await?;
        let class = classes
            .first()
            .ok_or_else(|| format!("class not found: {}", class_pattern))?;

        let fields = session
            .connection
            .get_fields(class.type_id)
            .await
            .map_err(|e| format!("Failed to get fields: {}", e))?;

        let field = fields
            .iter()
            .find(|f| f.name == field_name)
            .ok_or_else(|| format!("field '{}' not found", field_name))?;

        let request_id = session
            .connection
            .set_field_watch(
                class.type_id,
                field.field_id,
                jdwp_client::SuspendPolicy::All,
            )
            .await
            .map_err(|e| format!("Failed to set watchpoint: {}", e))?;

        Ok(format!(
            "watch req={} {}.{} (on modification)",
            request_id, class_pattern, field_name
        ))
    }

    /// Evaluate a conditional breakpoint expression using a connection clone (no lock held).
    /// Returns true if the condition does NOT match (should skip/resume).
    async fn eval_condition(
        conn: &jdwp_client::JdwpConnection,
        thread_id: u64,
        var_name: &str,
        expected: &str,
    ) -> bool {
        let mut conn = conn.clone();

        let frames = match conn.get_frames(thread_id, 0, 1).await {
            Ok(f) if !f.is_empty() => f,
            _ => return false,
        };
        let frame = &frames[0];

        let var_table = match conn
            .get_variable_table(frame.location.class_id, frame.location.method_id)
            .await
        {
            Ok(vt) => vt,
            Err(_) => return false,
        };

        let ci = frame.location.index;
        let var = match var_table
            .iter()
            .filter(|v| ci >= v.code_index && ci < v.code_index + v.length as u64)
            .find(|v| v.name == var_name)
        {
            Some(v) => v,
            None => return false,
        };

        let slots = vec![jdwp_client::stackframe::VariableSlot {
            slot: var.slot as i32,
            sig_byte: var.signature.as_bytes().first().copied().unwrap_or(b'L'),
        }];

        let values = match conn
            .get_frame_values(thread_id, frame.frame_id, slots)
            .await
        {
            Ok(v) => v,
            Err(_) => return false,
        };

        let actual = match values.first() {
            Some(v) => v.format_compact(),
            None => return false,
        };

        actual.trim_matches('"') != expected.trim_matches('"')
    }

    /// Collect trace events into session trace state. Returns true if events were trace-only.
    async fn collect_trace_events(
        session_manager: &SessionManager,
        event_set: &jdwp_client::EventSet,
    ) -> bool {
        let session_guard = match session_manager.get_current_session().await {
            Some(g) => g,
            None => return false,
        };
        let mut session = session_guard.lock().await;

        let (entry_req, exit_req, max) = match session.trace_state.as_ref().filter(|t| t.active) {
            Some(t) => (t.entry_request_id, t.exit_request_id, t.max_calls),
            None => return false,
        };

        // Phase 1: extract event data (no mutable borrows)
        let mut collected = false;
        let mut raw: Vec<(u64, u64, u64, bool)> = Vec::new();
        for event in &event_set.events {
            let is_entry = event.request_id == entry_req;
            let is_exit = event.request_id == exit_req;
            if !is_entry && !is_exit {
                continue;
            }
            collected = true;
            match &event.details {
                jdwp_client::events::EventKind::MethodEntry { thread, location } => {
                    raw.push((*thread, location.class_id, location.method_id, true));
                }
                jdwp_client::events::EventKind::MethodExit { thread, .. } => {
                    raw.push((*thread, 0, 0, false));
                }
                _ => {}
            }
        }

        if !collected {
            return false;
        }

        // Phase 2: mutate trace state (store raw IDs, resolve names at output time)
        let trace = session.trace_state.as_mut().unwrap();
        for (thread, class_id, method_id, is_entry) in raw {
            if trace.calls.len() >= max {
                trace.active = false;
                break;
            }
            if is_entry {
                let depth = trace.depth_per_thread.entry(thread).or_insert(0);
                trace.calls.push(crate::session::TraceCall {
                    thread_id: thread,
                    class_id,
                    method_id,
                    depth: *depth,
                    result: crate::session::TraceResult::Entry,
                });
                *depth += 1;
            } else {
                let depth = trace.depth_per_thread.entry(thread).or_insert(0);
                if *depth > 0 {
                    *depth -= 1;
                }
                if let Some(call) = trace.calls.iter_mut().rev().find(|c| {
                    c.thread_id == thread
                        && c.depth == *depth
                        && matches!(c.result, crate::session::TraceResult::Entry)
                }) {
                    call.result = crate::session::TraceResult::Returned(None);
                }
            }
        }
        if trace.start_time.elapsed().as_secs() > 60 {
            trace.active = false;
        }
        true
    }

    async fn handle_trace(&self, args: serde_json::Value) -> Result<String, String> {
        let class_pattern = args
            .get("class_pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'class_pattern'".to_string())?;
        let include_args = args
            .get("include_args")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;
        let mut session = session_guard.lock().await;

        // Clear any existing trace
        if let Some(old_trace) = session.trace_state.take() {
            if old_trace.active {
                let _ = session
                    .connection
                    .clear_method_entry_trace(old_trace.entry_request_id)
                    .await;
                let _ = session
                    .connection
                    .clear_method_exit_trace(old_trace.exit_request_id)
                    .await;
            }
        }

        // Convert dot notation to JDWP glob: com.example.service → com.example.service.*
        let pattern = if class_pattern.contains('*') {
            class_pattern.to_string()
        } else {
            format!("{}*", class_pattern)
        };

        let suspend = if include_args {
            jdwp_client::SuspendPolicy::EventThread
        } else {
            jdwp_client::SuspendPolicy::None
        };

        let entry_id = session
            .connection
            .set_method_entry_trace(&pattern, suspend)
            .await
            .map_err(|e| format!("Failed to set method entry trace: {}", e))?;

        let exit_id = session
            .connection
            .set_method_exit_trace(&pattern, suspend)
            .await
            .map_err(|e| format!("Failed to set method exit trace: {}", e))?;

        session.trace_state = Some(crate::session::TraceState {
            active: true,
            entry_request_id: entry_id,
            exit_request_id: exit_id,
            include_args,
            calls: Vec::new(),
            depth_per_thread: std::collections::HashMap::new(),
            start_time: std::time::Instant::now(),
            max_calls: 500,
        });

        Ok(format!(
            "tracing {}* armed (entry={}, exit={})",
            class_pattern, entry_id, exit_id
        ))
    }

    async fn handle_trace_result(&self, args: serde_json::Value) -> Result<String, String> {
        let clear = args.get("clear").and_then(|v| v.as_bool()).unwrap_or(true);

        let session_guard = self
            .session_manager
            .get_current_session()
            .await
            .ok_or_else(|| "No active debug session".to_string())?;
        let mut session = session_guard.lock().await;

        // Take trace state out to avoid borrow conflicts
        let trace = session
            .trace_state
            .take()
            .ok_or_else(|| "No active trace. Use debug.trace first.".to_string())?;

        let elapsed = trace.start_time.elapsed().as_millis();
        let call_count = trace.calls.len();
        let truncated = !trace.active && call_count >= trace.max_calls;

        // Build output
        let output = if call_count == 0 {
            format!(
                "trace: 0 calls in {}ms (no matching methods hit)\n",
                elapsed
            )
        } else {
            let mut out = format!("trace: {} calls, {}ms\n", call_count, elapsed);

            // Resolve class and method names
            for (idx, call) in trace.calls.iter().enumerate() {
                let indent = "  ".repeat(call.depth as usize);

                // Resolve class name
                let class_name = Self::get_class_signature(&mut session, call.class_id)
                    .await
                    .and_then(|sig| Self::signature_to_display_name(&sig))
                    .unwrap_or_else(|| format!("0x{:x}", call.class_id));

                // Resolve method name
                let method_name =
                    if let Ok(methods) = session.connection.get_methods(call.class_id).await {
                        methods
                            .iter()
                            .find(|m| m.method_id == call.method_id)
                            .map(|m| m.name.clone())
                            .unwrap_or_else(|| format!("0x{:x}", call.method_id))
                    } else {
                        format!("0x{:x}", call.method_id)
                    };

                let result_suffix = match &call.result {
                    crate::session::TraceResult::Returned(Some(v)) => format!(" -> {}", v),
                    crate::session::TraceResult::ThrewException(e) => format!(" -> threw {}", e),
                    _ => String::new(),
                };
                out.push_str(&format!(
                    "#{} {}{}.{}{}\n",
                    idx + 1,
                    indent,
                    class_name,
                    method_name,
                    result_suffix
                ));
            }
            if truncated {
                out.push_str("(truncated at 500 calls)\n");
            }
            out
        };

        // Clear or put back
        if clear {
            let _ = session
                .connection
                .clear_method_entry_trace(trace.entry_request_id)
                .await;
            let _ = session
                .connection
                .clear_method_exit_trace(trace.exit_request_id)
                .await;
            // trace_state already taken out (None)
        } else {
            session.trace_state = Some(trace);
        }

        Ok(output)
    }
}
