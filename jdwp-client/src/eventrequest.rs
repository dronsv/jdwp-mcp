// EventRequest command implementations
//
// Set up event requests (breakpoints, steps, exceptions, etc.)

use crate::commands::{command_sets, event_commands, event_kinds, step_depths, step_sizes};
use crate::connection::JdwpConnection;
use crate::protocol::{CommandPacket, JdwpResult};
use crate::reader::read_i32;
use crate::types::{MethodId, ReferenceTypeId, ThreadId};
use bytes::BufMut;

/// Suspend policy for events
#[repr(u8)]
pub enum SuspendPolicy {
    None = 0,
    EventThread = 1,
    All = 2,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy)]
pub enum StepSize {
    Min = step_sizes::MIN,
    Line = step_sizes::LINE,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy)]
pub enum StepDepth {
    Into = step_depths::INTO,
    Over = step_depths::OVER,
    Out = step_depths::OUT,
}

impl JdwpConnection {
    /// Set a breakpoint at a specific location (EventRequest.Set command)
    /// Returns the request ID for this breakpoint
    pub async fn set_breakpoint(
        &mut self,
        class_id: ReferenceTypeId,
        method_id: MethodId,
        bytecode_index: u64,
        suspend_policy: SuspendPolicy,
    ) -> JdwpResult<i32> {
        let id = self.next_id();
        let mut packet = CommandPacket::new(id, command_sets::EVENT_REQUEST, event_commands::SET);

        // Event kind: BREAKPOINT (2)
        packet.data.put_u8(event_kinds::BREAKPOINT);

        // Suspend policy
        packet.data.put_u8(suspend_policy as u8);

        // Number of modifiers (1 - location only)
        packet.data.put_i32(1);

        // Modifier kind: LocationOnly (7)
        packet.data.put_u8(7);

        // Location:
        // - type tag (1 = class)
        packet.data.put_u8(1);
        // - class ID
        packet.data.put_u64(class_id);
        // - method ID
        packet.data.put_u64(method_id);
        // - index (bytecode position)
        packet.data.put_u64(bytecode_index);

        let reply = self.send_command(packet).await?;
        reply.check_error()?;

        let mut data = reply.data();
        let request_id = read_i32(&mut data)?;

        Ok(request_id)
    }

    /// Set a single-step event request for a specific thread.
    /// Returns the request ID for this step request.
    pub async fn set_step(
        &mut self,
        thread_id: ThreadId,
        size: StepSize,
        depth: StepDepth,
        suspend_policy: SuspendPolicy,
    ) -> JdwpResult<i32> {
        let id = self.next_id();
        let mut packet = CommandPacket::new(id, command_sets::EVENT_REQUEST, event_commands::SET);

        // Event kind: SINGLE_STEP (1)
        packet.data.put_u8(event_kinds::SINGLE_STEP);

        // Suspend policy
        packet.data.put_u8(suspend_policy as u8);

        // Number of modifiers (1 - step modifier)
        packet.data.put_i32(1);

        // Modifier kind: Step (10)
        packet.data.put_u8(10);
        packet.data.put_u64(thread_id);
        packet.data.put_i32(size as i32);
        packet.data.put_i32(depth as i32);

        let reply = self.send_command(packet).await?;
        reply.check_error()?;

        let mut data = reply.data();
        let request_id = read_i32(&mut data)?;

        Ok(request_id)
    }

    /// Clear an event request by request ID and event kind.
    pub async fn clear_event_request(&mut self, event_kind: u8, request_id: i32) -> JdwpResult<()> {
        let id = self.next_id();
        let mut packet = CommandPacket::new(id, command_sets::EVENT_REQUEST, event_commands::CLEAR);

        // Event kind
        packet.data.put_u8(event_kind);

        // Request ID
        packet.data.put_i32(request_id);

        let reply = self.send_command(packet).await?;
        reply.check_error()?;

        Ok(())
    }

    /// Clear a breakpoint by request ID (EventRequest.Clear command)
    pub async fn clear_breakpoint(&mut self, request_id: i32) -> JdwpResult<()> {
        self.clear_event_request(event_kinds::BREAKPOINT, request_id)
            .await
    }

    /// Clear a single-step request by request ID.
    pub async fn clear_step(&mut self, request_id: i32) -> JdwpResult<()> {
        self.clear_event_request(event_kinds::SINGLE_STEP, request_id)
            .await
    }
}
