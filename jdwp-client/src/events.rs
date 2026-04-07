// JDWP event handling
//
// Events are sent from the JVM to notify about breakpoints, steps, etc.

use crate::commands::event_kinds;
use crate::protocol::{JdwpError, JdwpResult};
use crate::reader::{read_i32, read_string, read_u64, read_u8};
use crate::types::*;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// Composite event packet (can contain multiple events)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSet {
    pub suspend_policy: u8,
    pub events: Vec<Event>,
}

/// Single event within an event set
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub kind: u8,
    pub request_id: i32,
    pub details: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventKind {
    VMStart {
        thread: ThreadId,
    },
    VMDeath,
    ThreadStart {
        thread: ThreadId,
    },
    ThreadDeath {
        thread: ThreadId,
    },
    ClassPrepare {
        thread: ThreadId,
        ref_type: ReferenceTypeId,
        signature: String,
        status: i32,
    },
    Breakpoint {
        thread: ThreadId,
        location: Location,
    },
    Step {
        thread: ThreadId,
        location: Location,
    },
    Exception {
        thread: ThreadId,
        location: Location,
        exception: ObjectId,
        catch_location: Option<Location>,
    },
    MethodEntry {
        thread: ThreadId,
        location: Location,
    },
    MethodExit {
        thread: ThreadId,
        location: Location,
    },
    Unknown {
        kind: u8,
    },
}

// Event request modifiers
#[derive(Debug, Clone)]
pub enum EventModifier {
    Count(i32),
    ThreadOnly(ThreadId),
    ClassOnly(ReferenceTypeId),
    ClassMatch(String),
    ClassExclude(String),
    LocationOnly(Location),
    ExceptionOnly {
        ref_type: ReferenceTypeId,
        caught: bool,
        uncaught: bool,
    },
    FieldOnly {
        ref_type: ReferenceTypeId,
        field_id: FieldId,
    },
    Step {
        thread: ThreadId,
        size: i32,
        depth: i32,
    },
    InstanceOnly(ObjectId),
}

/// Parse an event packet from JDWP
pub fn parse_event_packet(data: &[u8]) -> JdwpResult<EventSet> {
    let mut buf = data;

    // Read suspend policy
    let suspend_policy = read_u8(&mut buf)?;

    // Read number of events
    let event_count = read_i32(&mut buf)?;

    let mut events = Vec::with_capacity((event_count as usize).min(64));

    for _ in 0..event_count {
        let kind = read_u8(&mut buf)?;
        let request_id = read_i32(&mut buf)?;

        let details = match kind {
            event_kinds::BREAKPOINT => {
                let thread = read_u64(&mut buf)?;
                let location = read_location(&mut buf)?;
                EventKind::Breakpoint { thread, location }
            }
            event_kinds::SINGLE_STEP => {
                let thread = read_u64(&mut buf)?;
                let location = read_location(&mut buf)?;
                EventKind::Step { thread, location }
            }
            event_kinds::VM_START => {
                let thread = read_u64(&mut buf)?;
                EventKind::VMStart { thread }
            }
            event_kinds::VM_DEATH => EventKind::VMDeath,
            event_kinds::THREAD_START => {
                let thread = read_u64(&mut buf)?;
                EventKind::ThreadStart { thread }
            }
            event_kinds::THREAD_DEATH => {
                let thread = read_u64(&mut buf)?;
                EventKind::ThreadDeath { thread }
            }
            event_kinds::CLASS_PREPARE => {
                let thread = read_u64(&mut buf)?;
                let _ref_type_tag = read_u8(&mut buf)?;
                let ref_type = read_u64(&mut buf)?;
                let signature = read_string(&mut buf)?;
                let status = read_i32(&mut buf)?;
                EventKind::ClassPrepare {
                    thread,
                    ref_type,
                    signature,
                    status,
                }
            }
            event_kinds::EXCEPTION => {
                let thread = read_u64(&mut buf)?;
                let location = read_location(&mut buf)?;
                let _exception_tag = read_u8(&mut buf)?;
                let exception = read_u64(&mut buf)?;
                let catch_location = read_location(&mut buf)?;
                let catch_location = if catch_location.class_id == 0
                    && catch_location.method_id == 0
                    && catch_location.index == 0
                {
                    None
                } else {
                    Some(catch_location)
                };
                EventKind::Exception {
                    thread,
                    location,
                    exception,
                    catch_location,
                }
            }
            event_kinds::METHOD_ENTRY => {
                let thread = read_u64(&mut buf)?;
                let location = read_location(&mut buf)?;
                EventKind::MethodEntry { thread, location }
            }
            event_kinds::METHOD_EXIT | event_kinds::METHOD_EXIT_WITH_RETURN_VALUE => {
                let thread = read_u64(&mut buf)?;
                let location = read_location(&mut buf)?;
                // METHOD_EXIT_WITH_RETURN_VALUE has an extra tagged value;
                // skip it by reading tag + value bytes
                if kind == event_kinds::METHOD_EXIT_WITH_RETURN_VALUE {
                    let value_tag = read_u8(&mut buf)?;
                    // Skip the value data based on tag size
                    let skip_bytes: usize = match value_tag {
                        86 => 0,                                        // void
                        66 | 90 => 1,                                   // byte, boolean
                        67 | 83 => 2,                                   // char, short
                        70 | 73 => 4,                                   // float, int
                        68 | 74 => 8,                                   // double, long
                        76 | 115 | 116 | 103 | 108 | 99 | 91 => 8,     // object types
                        _ => 0,
                    };
                    if skip_bytes > 0 {
                        if buf.len() < skip_bytes {
                            return Err(JdwpError::Protocol(
                                "Not enough data for method exit return value".to_string(),
                            ));
                        }
                        buf = &buf[skip_bytes..];
                    }
                }
                EventKind::MethodExit { thread, location }
            }
            _ => {
                warn!("Unsupported event kind: {}, cannot parse remaining events in composite packet", kind);
                // Cannot safely continue — we don't know how many bytes this event occupies
                break;
            }
        };

        events.push(Event {
            kind,
            request_id,
            details,
        });
    }

    Ok(EventSet {
        suspend_policy,
        events,
    })
}

/// Read a location from the buffer
fn read_location(buf: &mut &[u8]) -> JdwpResult<Location> {
    let type_tag = read_u8(buf)?;
    let class_id = read_u64(buf)?;
    let method_id = read_u64(buf)?;
    let index = read_u64(buf)?;

    Ok(Location {
        type_tag,
        class_id,
        method_id,
        index,
    })
}
