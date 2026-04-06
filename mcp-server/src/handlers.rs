// MCP request handlers
//
// Handles initialize, list tools, and debug tool execution

use crate::protocol::*;
use crate::session::{SessionManager, StepRequestInfo};
use crate::tools;
use jdwp_client::vm::ClassInfo;
use serde_json::json;
use tracing::{debug, info, warn};

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
            instructions: Some(
                "JDWP debugging server for Java applications. \
                Start by using debug.attach to connect to a JVM, \
                then use debug.set_breakpoint, debug.get_stack, etc."
                    .to_string(),
            ),
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
        let port = args.get("port").and_then(|v| v.as_u64()).unwrap_or(5005) as u16;
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

                                if let Some(session_guard) =
                                    session_manager.get_current_session().await
                                {
                                    let mut session = session_guard.lock().await;
                                    if step_request_to_clear
                                        == session.active_step.as_ref().map(|s| s.request_id)
                                    {
                                        session.active_step = None;
                                    }
                                    session.last_event = Some(event_set);
                                    session.last_event_seq += 1;
                                    session.last_event_notify.notify_waiters();
                                } else {
                                    break; // Session gone
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
                    "Connected to JVM at {}:{} (session: {})",
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
        let bp_id = format!("bp_{}", request_id);
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
            },
        );

        Ok(format!(
            "✅ Breakpoint set at {}:{}\n   Resolved class: {}\n   Method: {}\n   Breakpoint ID: {}\n   JDWP Request ID: {}",
            class_pattern,
            line,
            class.signature,
            method.name,
            bp_id,
            request_id
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
            return Ok("No breakpoints set".to_string());
        }

        let mut output = format!("📍 {} breakpoint(s):\n\n", session.breakpoints.len());

        for (_, bp) in session.breakpoints.iter() {
            output.push_str(&format!(
                "  {} [{}] {}:{}\n",
                if bp.enabled { "✓" } else { "✗" },
                bp.id,
                bp.class_pattern,
                bp.line
            ));
            if let Some(method) = &bp.method {
                output.push_str(&format!("     Method: {}\n", method));
            }
            if bp.hit_count > 0 {
                output.push_str(&format!("     Hits: {}\n", bp.hit_count));
            }
        }

        if let Some(active_step) = &session.active_step {
            output.push_str(&format!(
                "\n  → active step: {} on thread 0x{:x} (request_id={})\n",
                active_step.depth, active_step.thread_id, active_step.request_id
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
            "✅ Breakpoint cleared: {} at {}:{}\n   JDWP Request ID: {}",
            bp_id, bp_info.class_pattern, bp_info.line, bp_info.request_id
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

        Ok("▶️  Execution resumed".to_string())
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
            | jdwp_client::events::EventKind::MethodExit { thread, .. } => Some(*thread),
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
            "⏭️  Step {} armed on thread 0x{:x}\n   Request ID: {}\n   Size: line\n   Execution resumed",
            depth_label,
            target_thread,
            request_id
        ))
    }

    async fn format_value(
        session: &mut crate::session::DebugSession,
        value: &jdwp_client::types::Value,
    ) -> String {
        if value.tag == 115 {
            if let jdwp_client::types::ValueData::Object(object_id) = &value.data {
                if *object_id != 0 {
                    return match session.connection.get_string_value(*object_id).await {
                        Ok(string_val) => format!("(String) \"{}\"", string_val),
                        Err(_) => value.format(),
                    };
                }
                return "(String) null".to_string();
            }
        }
        value.format()
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
            .and_then(|v| v.as_i64())
            .unwrap_or(20) as usize;

        let include_variables = args
            .get("include_variables")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

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
            return Ok(format!("Thread {:x} has no stack frames", target_thread));
        }

        let mut output = format!(
            "🔍 Stack for thread {:x} ({} frames):\n\n",
            target_thread,
            frames.len()
        );

        for (idx, frame) in frames.iter().enumerate() {
            output.push_str(&format!("Frame {}:\n", idx));
            if let Some(signature) =
                Self::get_class_signature(&mut session, frame.location.class_id).await
            {
                let display = Self::signature_to_display_name(&signature)
                    .unwrap_or_else(|| signature.clone());
                output.push_str(&format!("  Class: {} ({})\n", display, signature));
            }
            output.push_str(&format!(
                "  Location: class={:x}, method={:x}, index={}\n",
                frame.location.class_id, frame.location.method_id, frame.location.index
            ));

            // Try to get method name
            if let Ok(methods) = session
                .connection
                .get_methods(frame.location.class_id)
                .await
            {
                if let Some(method) = methods
                    .iter()
                    .find(|m| m.method_id == frame.location.method_id)
                {
                    output.push_str(&format!("  Method: {}\n", method.name));

                    // Get variables if requested
                    if include_variables {
                        match session
                            .connection
                            .get_variable_table(frame.location.class_id, frame.location.method_id)
                            .await
                        {
                            Ok(var_table) => {
                                let current_index = frame.location.index;
                                let active_vars: Vec<_> = var_table
                                    .iter()
                                    .filter(|v| {
                                        current_index >= v.code_index
                                            && current_index < v.code_index + v.length as u64
                                    })
                                    .collect();

                                if !active_vars.is_empty() {
                                    output.push_str(&format!(
                                        "  Variables ({}):\n",
                                        active_vars.len()
                                    ));

                                    let slots: Vec<jdwp_client::stackframe::VariableSlot> =
                                        active_vars
                                            .iter()
                                            .map(|v| jdwp_client::stackframe::VariableSlot {
                                                slot: v.slot as i32,
                                                sig_byte: v.signature.as_bytes()[0],
                                            })
                                            .collect();

                                    if let Ok(values) = session
                                        .connection
                                        .get_frame_values(target_thread, frame.frame_id, slots)
                                        .await
                                    {
                                        for (var, value) in active_vars.iter().zip(values.iter()) {
                                            let formatted_value =
                                                Self::format_value(&mut session, value).await;
                                            output.push_str(&format!(
                                                "    {} = {}\n",
                                                var.name, formatted_value
                                            ));
                                        }
                                    }
                                }
                            }
                            Err(_) => {}
                        }
                    }
                }
            }

            output.push_str("\n");
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

        let methods = session
            .connection
            .get_methods(frame.location.class_id)
            .await
            .map_err(|e| format!("Failed to get methods: {}", e))?;
        let method = methods
            .iter()
            .find(|m| m.method_id == frame.location.method_id);

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
            sig_byte: var.signature.as_bytes()[0],
        }];
        let values = session
            .connection
            .get_frame_values(target_thread, frame.frame_id, slots)
            .await
            .map_err(|e| format!("Failed to get frame value: {}", e))?;
        let value = values
            .first()
            .ok_or_else(|| "JDWP returned no variable value".to_string())?;
        let formatted_value = Self::format_value(&mut session, value).await;

        Ok(format!(
            "Variable {} in frame {} on thread 0x{:x}\n  Method: {}\n  Signature: {}\n  Value: {}",
            name,
            frame_index,
            target_thread,
            method.map(|m| m.name.as_str()).unwrap_or("<unknown>"),
            var.signature,
            formatted_value
        ))
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
        Ok(format!(
            "Selected thread 0x{:x} for subsequent inspection",
            thread_id
        ))
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

        let mut output = format!("🧵 {} thread(s):\n\n", threads.len());

        for (idx, thread_id) in threads.iter().enumerate() {
            let mut markers = Vec::new();
            if selected_thread_id == Some(*thread_id) {
                markers.push("selected");
            }
            if event_thread_id == Some(*thread_id) {
                markers.push("last-event");
            }
            let marker_text = if markers.is_empty() {
                String::new()
            } else {
                format!(" [{}]", markers.join(", "))
            };

            output.push_str(&format!(
                "  Thread {} (ID: 0x{:x}){}\n",
                idx + 1,
                thread_id,
                marker_text
            ));

            // Try to get frame count
            match session.connection.get_frames(*thread_id, 0, 1).await {
                Ok(frames) if !frames.is_empty() => {
                    output.push_str("     Status: Has frames (possibly suspended)\n");
                }
                Ok(_) => {
                    output.push_str("     Status: Running (no frames)\n");
                }
                Err(_) => {
                    output.push_str("     Status: Cannot inspect\n");
                }
            }
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

        Ok("⏸️  Execution paused (all threads suspended)".to_string())
    }

    async fn handle_disconnect(&self, _args: serde_json::Value) -> Result<String, String> {
        let current_session_id = self.session_manager.get_current_session_id().await;

        if let Some(session_id) = current_session_id {
            // Remove the session (this will also clear current session)
            self.session_manager.remove_session(&session_id).await;
            Ok(format!(
                "✅ Disconnected from debug session: {}",
                session_id
            ))
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
            let mut output = format!(
                "🎯 Last event (suspend_policy={})\n\n",
                event_set.suspend_policy
            );

            for (idx, event) in event_set.events.iter().enumerate() {
                output.push_str(&format!("Event {}:\n", idx + 1));
                output.push_str(&format!("  Request ID: {}\n", event.request_id));

                match &event.details {
                    jdwp_client::events::EventKind::Breakpoint { thread, location } => {
                        output.push_str("  Type: Breakpoint\n");
                        output.push_str(&format!("  ⚡ Thread ID: 0x{:x}\n", thread));
                        output.push_str(&format!(
                            "  Location: class=0x{:x}, method=0x{:x}, index={}\n",
                            location.class_id, location.method_id, location.index
                        ));
                    }
                    jdwp_client::events::EventKind::Step { thread, location } => {
                        output.push_str("  Type: Step\n");
                        output.push_str(&format!("  Thread ID: 0x{:x}\n", thread));
                        output.push_str(&format!(
                            "  Location: class=0x{:x}, method=0x{:x}, index={}\n",
                            location.class_id, location.method_id, location.index
                        ));
                    }
                    jdwp_client::events::EventKind::VMStart { thread } => {
                        output.push_str("  Type: VM Start\n");
                        output.push_str(&format!("  Thread ID: 0x{:x}\n", thread));
                    }
                    jdwp_client::events::EventKind::VMDeath => {
                        output.push_str("  Type: VM Death\n");
                    }
                    jdwp_client::events::EventKind::ThreadStart { thread } => {
                        output.push_str("  Type: Thread Start\n");
                        output.push_str(&format!("  Thread ID: 0x{:x}\n", thread));
                    }
                    jdwp_client::events::EventKind::ThreadDeath { thread } => {
                        output.push_str("  Type: Thread Death\n");
                        output.push_str(&format!("  Thread ID: 0x{:x}\n", thread));
                    }
                    jdwp_client::events::EventKind::ClassPrepare {
                        thread,
                        ref_type,
                        signature,
                        ..
                    } => {
                        output.push_str("  Type: Class Prepare\n");
                        output.push_str(&format!("  Thread ID: 0x{:x}\n", thread));
                        output.push_str(&format!("  Class: {} (0x{:x})\n", signature, ref_type));
                    }
                    _ => {
                        output.push_str("  Type: Other\n");
                    }
                }

                output.push_str("\n");
            }

            Ok(output)
        } else {
            Ok("No events received yet. Set a breakpoint and trigger it.".to_string())
        }
    }

    async fn handle_wait_for_event(&self, args: serde_json::Value) -> Result<String, String> {
        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(30000);
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
                    notify.notified().await;
                    let session = session_guard.lock().await;
                    if session.last_event_seq > start_seq {
                        return session.last_event.clone();
                    }
                }
            })
            .await;

        match wait_result {
            Ok(Some(event_set)) => {
                let thread_text = Self::event_thread_id(&event_set)
                    .map(|tid| format!(" thread=0x{:x}", tid))
                    .unwrap_or_default();
                Ok(format!(
                    "Received event: count={} suspend_policy={}{}",
                    event_set.events.len(),
                    event_set.suspend_policy,
                    thread_text
                ))
            }
            Ok(None) => Err("Wait completed but no event was captured".to_string()),
            Err(_) => Err(format!(
                "Timed out waiting for event after {}ms",
                timeout_ms
            )),
        }
    }
}
