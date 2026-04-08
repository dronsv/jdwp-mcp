// Debug tools schema definitions

use crate::protocol::Tool;
use serde_json::json;

pub fn get_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "debug.attach".to_string(),
            description: "Connect to JVM via JDWP".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "host": { "type": "string", "default": "localhost" },
                    "port": { "type": "integer", "default": 5005 },
                    "timeout_ms": { "type": "integer", "default": 5000 },
                    "allow_remote": { "type": "boolean", "default": false }
                },
                "required": ["host", "port"]
            }),
        },
        Tool {
            name: "debug.set_breakpoint".to_string(),
            description: "Set breakpoint at class:line, optionally conditional".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "class_pattern": { "type": "string", "description": "e.g. com.example.MyClass" },
                    "line": { "type": "integer" },
                    "method": { "type": "string", "description": "optional, disambiguates" },
                    "condition": { "type": "string", "description": "e.g. count==5 — auto-resumes if false" }
                },
                "required": ["class_pattern", "line"]
            }),
        },
        Tool {
            name: "debug.list_breakpoints".to_string(),
            description: "List active breakpoints".to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "debug.clear_breakpoint".to_string(),
            description: "Remove a breakpoint by ID".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "breakpoint_id": { "type": "string" }
                },
                "required": ["breakpoint_id"]
            }),
        },
        Tool {
            name: "debug.continue".to_string(),
            description: "Resume all threads".to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "debug.step_into".to_string(),
            description: "Step into next line".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "hex e.g. 0x1a2b, optional" }
                }
            }),
        },
        Tool {
            name: "debug.step_over".to_string(),
            description: "Step over next line".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "hex e.g. 0x1a2b, optional" }
                }
            }),
        },
        Tool {
            name: "debug.step_out".to_string(),
            description: "Step out of current frame".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "hex e.g. 0x1a2b, optional" }
                }
            }),
        },
        Tool {
            name: "debug.get_stack".to_string(),
            description: "Get stack frames with variables".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string" },
                    "max_frames": { "type": "integer", "default": 20 },
                    "include_variables": { "type": "boolean", "default": true },
                    "max_variable_depth": { "type": "integer", "default": 2 }
                }
            }),
        },
        Tool {
            name: "debug.get_variable".to_string(),
            description: "Read one local variable by name".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "thread_id": { "type": "string", "description": "hex, optional" },
                    "frame_index": { "type": "integer", "default": 0 }
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "debug.select_thread".to_string(),
            description: "Set default thread for inspection".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "hex e.g. 0x1a2b" }
                },
                "required": ["thread_id"]
            }),
        },
        Tool {
            name: "debug.list_threads".to_string(),
            description: "List all threads".to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "debug.pause".to_string(),
            description: "Suspend all threads".to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "debug.disconnect".to_string(),
            description: "End debug session".to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "debug.get_last_event".to_string(),
            description: "Show last breakpoint/step event".to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "debug.wait_for_event".to_string(),
            description: "Wait for next event with timeout".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "timeout_ms": { "type": "integer", "default": 30000 }
                }
            }),
        },
        Tool {
            name: "debug.inspect".to_string(),
            description: "Inspect object fields by object ID".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "object_id": { "type": "string", "description": "hex e.g. 0x1a3f" }
                },
                "required": ["object_id"]
            }),
        },
        Tool {
            name: "debug.find_class".to_string(),
            description: "Search loaded classes by name pattern".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "e.g. UserService or com.example.User" }
                },
                "required": ["pattern"]
            }),
        },
        Tool {
            name: "debug.list_methods".to_string(),
            description: "List methods of a class with line ranges".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "class_pattern": { "type": "string", "description": "e.g. com.example.MyClass" }
                },
                "required": ["class_pattern"]
            }),
        },
        Tool {
            name: "debug.exception_breakpoint".to_string(),
            description: "Break on exceptions (caught/uncaught, optionally by class)".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "class_pattern": { "type": "string", "description": "exception class, omit for all" },
                    "caught": { "type": "boolean", "default": true },
                    "uncaught": { "type": "boolean", "default": true }
                }
            }),
        },
        Tool {
            name: "debug.eval".to_string(),
            description: "Invoke a method on an object. Supports args: list.get(0), map.get(\"key\")".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "object_id": { "type": "string", "description": "hex e.g. 0x1a3f" },
                    "method": { "type": "string", "default": "toString" },
                    "args": { "type": "array", "description": "method arguments: [0], [\"key\"], [true]" },
                    "thread_id": { "type": "string", "description": "hex, optional" }
                },
                "required": ["object_id"]
            }),
        },
        Tool {
            name: "debug.set_value".to_string(),
            description: "Set a local variable value in a stack frame".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "value": { "description": "new value (int/bool/float/string/null)" },
                    "thread_id": { "type": "string", "description": "hex, optional" },
                    "frame_index": { "type": "integer", "default": 0 }
                },
                "required": ["name", "value"]
            }),
        },
        Tool {
            name: "debug.snapshot".to_string(),
            description: "Combined dump: last event + breakpoints + stack with vars".to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "debug.vm_info".to_string(),
            description: "Get JVM version info".to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "debug.watch".to_string(),
            description: "Watchpoint: break when a field is modified".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "class_pattern": { "type": "string", "description": "e.g. com.example.MyClass" },
                    "field": { "type": "string", "description": "field name to watch" }
                },
                "required": ["class_pattern", "field"]
            }),
        },
        Tool {
            name: "debug.trace".to_string(),
            description: "Trace method calls on a class/package. Returns immediately; use debug.trace_result to get the call path.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "class_pattern": { "type": "string", "description": "e.g. com.example.service (appends * automatically)" },
                    "include_args": { "type": "boolean", "default": false, "description": "capture method args (slower)" },
                    "aggregate": { "type": "boolean", "default": false, "description": "aggregate mode: count calls + wall-clock time per method" }
                },
                "required": ["class_pattern"]
            }),
        },
        Tool {
            name: "debug.trace_result".to_string(),
            description: "Get collected trace after debug.trace. Shows call path with depth.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "clear": { "type": "boolean", "default": true, "description": "stop tracing after retrieval" },
                    "min_ms": { "type": "integer", "default": 0, "description": "aggregate mode: only show methods >= this ms" }
                }
            }),
        },
        Tool {
            name: "debug.wait_for_class".to_string(),
            description: "Wait for a class to be loaded by the JVM. Use when set_breakpoint says class not found.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "class_pattern": { "type": "string", "description": "e.g. com.example.MyService or *MyService" },
                    "timeout_ms": { "type": "integer", "default": 30000 }
                },
                "required": ["class_pattern"]
            }),
        },
    ]
}
