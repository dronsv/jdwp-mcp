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
#[derive(Debug, Clone, Copy)]
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

    /// Set an exception breakpoint (EventRequest.Set with EXCEPTION kind).
    /// `exception_class_id` = 0 means all exceptions.
    /// Returns the request ID.
    pub async fn set_exception_breakpoint(
        &mut self,
        exception_class_id: ReferenceTypeId,
        caught: bool,
        uncaught: bool,
        suspend_policy: SuspendPolicy,
    ) -> JdwpResult<i32> {
        let id = self.next_id();
        let mut packet = CommandPacket::new(id, command_sets::EVENT_REQUEST, event_commands::SET);

        packet.data.put_u8(event_kinds::EXCEPTION);
        packet.data.put_u8(suspend_policy as u8);

        // 1 modifier: ExceptionOnly (8)
        packet.data.put_i32(1);
        packet.data.put_u8(8);
        packet.data.put_u64(exception_class_id);
        packet.data.put_u8(caught as u8);
        packet.data.put_u8(uncaught as u8);

        let reply = self.send_command(packet).await?;
        reply.check_error()?;

        let mut data = reply.data();
        let request_id = read_i32(&mut data)?;
        Ok(request_id)
    }

    /// Clear an exception breakpoint by request ID.
    pub async fn clear_exception_breakpoint(&mut self, request_id: i32) -> JdwpResult<()> {
        self.clear_event_request(event_kinds::EXCEPTION, request_id)
            .await
    }

    /// Set a field modification watchpoint (EventRequest.Set with FIELD_MODIFICATION).
    /// Fires when the specified field is written to.
    pub async fn set_field_watch(
        &mut self,
        class_id: ReferenceTypeId,
        field_id: crate::types::FieldId,
        suspend_policy: SuspendPolicy,
    ) -> JdwpResult<i32> {
        let id = self.next_id();
        let mut packet = CommandPacket::new(id, command_sets::EVENT_REQUEST, event_commands::SET);

        packet.data.put_u8(event_kinds::FIELD_MODIFICATION);
        packet.data.put_u8(suspend_policy as u8);

        // 1 modifier: FieldOnly (9)
        packet.data.put_i32(1);
        packet.data.put_u8(9);
        packet.data.put_u64(class_id);
        packet.data.put_u64(field_id);

        let reply = self.send_command(packet).await?;
        reply.check_error()?;

        let mut data = reply.data();
        let request_id = read_i32(&mut data)?;
        Ok(request_id)
    }

    /// Clear a field watchpoint by request ID.
    pub async fn clear_field_watch(&mut self, request_id: i32) -> JdwpResult<()> {
        self.clear_event_request(event_kinds::FIELD_MODIFICATION, request_id)
            .await
    }

    /// Set METHOD_ENTRY trace on classes matching a pattern.
    /// Uses ClassMatch modifier (kind=5) with a glob pattern like "com.example.*".
    /// Returns the request ID.
    pub async fn set_method_entry_trace(
        &mut self,
        class_pattern: &str,
        suspend_policy: SuspendPolicy,
    ) -> JdwpResult<i32> {
        let id = self.next_id();
        let mut packet = CommandPacket::new(id, command_sets::EVENT_REQUEST, event_commands::SET);

        packet.data.put_u8(event_kinds::METHOD_ENTRY);
        packet.data.put_u8(suspend_policy as u8);

        // 1 modifier: ClassMatch (5)
        packet.data.put_i32(1);
        packet.data.put_u8(5);
        crate::protocol::write_jdwp_string(&mut packet.data, class_pattern);

        let reply = self.send_command(packet).await?;
        reply.check_error()?;

        let mut data = reply.data();
        read_i32(&mut data)
    }

    /// Set METHOD_EXIT trace on classes matching a pattern.
    /// Returns the request ID.
    pub async fn set_method_exit_trace(
        &mut self,
        class_pattern: &str,
        suspend_policy: SuspendPolicy,
    ) -> JdwpResult<i32> {
        let id = self.next_id();
        let mut packet = CommandPacket::new(id, command_sets::EVENT_REQUEST, event_commands::SET);

        packet.data.put_u8(event_kinds::METHOD_EXIT);
        packet.data.put_u8(suspend_policy as u8);

        // 1 modifier: ClassMatch (5)
        packet.data.put_i32(1);
        packet.data.put_u8(5);
        crate::protocol::write_jdwp_string(&mut packet.data, class_pattern);

        let reply = self.send_command(packet).await?;
        reply.check_error()?;

        let mut data = reply.data();
        read_i32(&mut data)
    }

    /// Clear a method entry trace by request ID.
    pub async fn clear_method_entry_trace(&mut self, request_id: i32) -> JdwpResult<()> {
        self.clear_event_request(event_kinds::METHOD_ENTRY, request_id)
            .await
    }

    /// Clear a method exit trace by request ID.
    pub async fn clear_method_exit_trace(&mut self, request_id: i32) -> JdwpResult<()> {
        self.clear_event_request(event_kinds::METHOD_EXIT, request_id)
            .await
    }

    /// Set a CLASS_PREPARE event for classes matching a pattern.
    /// Fires when the JVM loads a class that matches. Use with SuspendPolicy::All
    /// to pause when the class loads so you can set breakpoints immediately.
    pub async fn set_class_prepare(
        &mut self,
        class_pattern: &str,
        suspend_policy: SuspendPolicy,
    ) -> JdwpResult<i32> {
        let id = self.next_id();
        let mut packet = CommandPacket::new(id, command_sets::EVENT_REQUEST, event_commands::SET);

        packet.data.put_u8(event_kinds::CLASS_PREPARE);
        packet.data.put_u8(suspend_policy as u8);

        // 1 modifier: ClassMatch (5)
        packet.data.put_i32(1);
        packet.data.put_u8(5);
        crate::protocol::write_jdwp_string(&mut packet.data, class_pattern);

        let reply = self.send_command(packet).await?;
        reply.check_error()?;

        let mut data = reply.data();
        read_i32(&mut data)
    }

    /// Clear a CLASS_PREPARE event by request ID.
    pub async fn clear_class_prepare(&mut self, request_id: i32) -> JdwpResult<()> {
        self.clear_event_request(event_kinds::CLASS_PREPARE, request_id)
            .await
    }
}
