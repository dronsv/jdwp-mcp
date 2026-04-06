// Debug tools schema definitions
//
// MCP tools for JDWP debugging operations

use crate::protocol::Tool;
use serde_json::json;

pub fn get_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "debug.attach".to_string(),
            description: "Connect to a JVM via JDWP protocol".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "host": {
                        "type": "string",
                        "description": "JVM host (e.g., 'localhost')",
                        "default": "localhost"
                    },
                    "port": {
                        "type": "integer",
                        "description": "JDWP port (e.g., 5005)",
                        "default": 5005
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Connection timeout in milliseconds",
                        "default": 5000
                    },
                    "allow_remote": {
                        "type": "boolean",
                        "description": "Allow attaching to a non-localhost host",
                        "default": false
                    }
                },
                "required": ["host", "port"]
            }),
        },
        Tool {
            name: "debug.set_breakpoint".to_string(),
            description: "Set a breakpoint at a specific location".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "class_pattern": {
                        "type": "string",
                        "description": "Class name pattern (e.g., 'com.example.MyClass')"
                    },
                    "line": {
                        "type": "integer",
                        "description": "Line number"
                    },
                    "method": {
                        "type": "string",
                        "description": "Method name (optional, helps resolve ambiguity)"
                    }
                },
                "required": ["class_pattern", "line"]
            }),
        },
        Tool {
            name: "debug.list_breakpoints".to_string(),
            description: "List all active breakpoints".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "debug.clear_breakpoint".to_string(),
            description: "Clear a specific breakpoint".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "breakpoint_id": {
                        "type": "string",
                        "description": "Breakpoint ID from list_breakpoints"
                    }
                },
                "required": ["breakpoint_id"]
            }),
        },
        Tool {
            name: "debug.continue".to_string(),
            description: "Resume execution for all threads".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "debug.step_into".to_string(),
            description: "Step into the next source line on a thread".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "thread_id": {
                        "type": "string",
                        "description": "Thread ID in hex form like 0x1a2b (optional; defaults to last event thread or selected thread)"
                    }
                }
            }),
        },
        Tool {
            name: "debug.step_over".to_string(),
            description: "Step over the next source line on a thread".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "thread_id": {
                        "type": "string",
                        "description": "Thread ID in hex form like 0x1a2b (optional; defaults to last event thread or selected thread)"
                    }
                }
            }),
        },
        Tool {
            name: "debug.step_out".to_string(),
            description: "Step out of the current frame on a thread".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "thread_id": {
                        "type": "string",
                        "description": "Thread ID in hex form like 0x1a2b (optional; defaults to last event thread or selected thread)"
                    }
                }
            }),
        },
        Tool {
            name: "debug.get_stack".to_string(),
            description: "Get stack frames with summarized variables".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "thread_id": {
                        "type": "string",
                        "description": "Thread ID"
                    },
                    "max_frames": {
                        "type": "integer",
                        "description": "Maximum number of frames to return",
                        "default": 20
                    },
                    "include_variables": {
                        "type": "boolean",
                        "description": "Include local variables in frames",
                        "default": true
                    },
                    "max_variable_depth": {
                        "type": "integer",
                        "description": "How deep to traverse object graphs (1-3)",
                        "default": 2
                    }
                }
            }),
        },
        Tool {
            name: "debug.get_variable".to_string(),
            description: "Get a single local variable by name from a stack frame".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Local variable name"
                    },
                    "thread_id": {
                        "type": "string",
                        "description": "Thread ID in hex form like 0x1a2b (optional; defaults to last event thread or first thread)"
                    },
                    "frame_index": {
                        "type": "integer",
                        "description": "Stack frame index (0 = current frame)",
                        "default": 0
                    }
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "debug.select_thread".to_string(),
            description: "Select a default thread for subsequent stack and variable inspection"
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "thread_id": {
                        "type": "string",
                        "description": "Thread ID in hex form like 0x1a2b"
                    }
                },
                "required": ["thread_id"]
            }),
        },
        Tool {
            name: "debug.list_threads".to_string(),
            description: "List all threads with status".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "debug.pause".to_string(),
            description: "Pause execution for all threads".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "debug.disconnect".to_string(),
            description: "Disconnect from JVM debug session".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "debug.get_last_event".to_string(),
            description: "Get the last breakpoint/event received with thread ID".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "debug.wait_for_event".to_string(),
            description: "Wait for the next breakpoint/event or time out".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Maximum time to wait in milliseconds",
                        "default": 30000
                    }
                }
            }),
        },
    ]
}
