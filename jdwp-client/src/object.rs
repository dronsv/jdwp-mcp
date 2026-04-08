// ObjectReference command implementations
//
// Commands for working with object instances

use crate::commands::{command_sets, object_reference_commands};
use crate::connection::JdwpConnection;
use crate::protocol::{CommandPacket, JdwpResult};
use crate::reader::{read_i32, read_u64, read_u8, read_value_by_tag};
use crate::types::{FieldId, ObjectId, ReferenceTypeId, Value};
use bytes::BufMut;
use serde::{Deserialize, Serialize};

/// Field value from an object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldValue {
    pub field_id: FieldId,
    pub value: Value,
}

impl JdwpConnection {
    /// Get the reference type (class) of an object (ObjectReference.ReferenceType command)
    ///
    /// # Arguments
    /// * `object_id` - The ObjectId of the object
    ///
    /// # Returns
    /// The ReferenceTypeId of the object's class
    pub async fn get_object_reference_type(
        &mut self,
        object_id: ObjectId,
    ) -> JdwpResult<ReferenceTypeId> {
        let id = self.next_id();
        let mut packet = CommandPacket::new(
            id,
            command_sets::OBJECT_REFERENCE,
            object_reference_commands::REFERENCE_TYPE,
        );

        packet.data.put_u64(object_id);

        let reply = self.send_command(packet).await?;
        reply.check_error()?;

        let mut data = reply.data();

        // Read type tag (byte) and class ID (objectID)
        let _type_tag = read_u8(&mut data)?;
        let reference_type_id = read_u64(&mut data)?;

        Ok(reference_type_id)
    }

    /// Get field values from an object (ObjectReference.GetValues command)
    ///
    /// # Arguments
    /// * `object_id` - The ObjectId of the object
    /// * `field_ids` - Vector of FieldIds to retrieve
    ///
    /// # Returns
    /// Vector of Values corresponding to the requested fields
    ///
    /// # Example
    /// ```ignore
    /// let fields = vec![field_id1, field_id2];
    /// let values = connection.get_object_values(object_id, fields).await?;
    /// ```
    pub async fn get_object_values(
        &mut self,
        object_id: ObjectId,
        field_ids: Vec<FieldId>,
    ) -> JdwpResult<Vec<Value>> {
        let id = self.next_id();
        let mut packet = CommandPacket::new(
            id,
            command_sets::OBJECT_REFERENCE,
            object_reference_commands::GET_VALUES,
        );

        // Write object ID
        packet.data.put_u64(object_id);

        // Write number of fields
        packet.data.put_i32(field_ids.len() as i32);

        // Write each field ID
        for field_id in &field_ids {
            packet.data.put_u64(*field_id);
        }

        let reply = self.send_command(packet).await?;
        reply.check_error()?;

        let mut data = reply.data();

        // Read number of values (should match field_ids.len())
        let values_count = read_i32(&mut data)?;
        let mut values = Vec::with_capacity((values_count as usize).min(1024));

        for _ in 0..values_count {
            let tag = read_u8(&mut data)?;
            let value_data = read_value_by_tag(tag, &mut data)?;

            values.push(Value {
                tag,
                data: value_data,
            });
        }

        Ok(values)
    }

    /// Invoke a method on an object (ObjectReference.InvokeMethod).
    /// Thread must be suspended. Returns (return_value, exception_object_id).
    /// exception_object_id = 0 means no exception was thrown.
    pub async fn invoke_method(
        &mut self,
        object_id: ObjectId,
        thread_id: crate::types::ThreadId,
        class_id: ReferenceTypeId,
        method_id: crate::types::MethodId,
        args: &[Value],
        single_threaded: bool,
    ) -> JdwpResult<(Value, ObjectId)> {
        let id = self.next_id();
        let mut packet = CommandPacket::new(
            id,
            command_sets::OBJECT_REFERENCE,
            object_reference_commands::INVOKE_METHOD,
        );

        packet.data.put_u64(object_id);
        packet.data.put_u64(thread_id);
        packet.data.put_u64(class_id);
        packet.data.put_u64(method_id);

        // arg count
        packet.data.put_i32(args.len() as i32);
        for arg in args {
            // tagged value: tag + value bytes
            packet.data.put_u8(arg.tag);
            arg.data.write_to(&mut packet.data);
        }

        // options: INVOKE_SINGLE_THREADED = 0x01
        let options: i32 = if single_threaded { 0x01 } else { 0x00 };
        packet.data.put_i32(options);

        let reply = self.send_command(packet).await?;
        reply.check_error()?;

        let mut data = reply.data();

        // Return value: tagged value
        let ret_tag = read_u8(&mut data)?;
        let ret_data = read_value_by_tag(ret_tag, &mut data)?;
        let return_value = Value {
            tag: ret_tag,
            data: ret_data,
        };

        // Exception: tagged-objectID
        let _exc_tag = read_u8(&mut data)?;
        let exception_id = read_u64(&mut data)?;

        Ok((return_value, exception_id))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_object_values_packet() {
        // Test that packet is constructed correctly
    }
}
