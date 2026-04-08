// ArrayReference command implementations

use crate::commands::{array_reference_commands, command_sets};
use crate::connection::JdwpConnection;
use crate::protocol::{CommandPacket, JdwpResult};
use crate::reader::{read_i32, read_u8};
use crate::types::{ArrayId, Value, ValueData};
use bytes::BufMut;

impl JdwpConnection {
    /// Get array length (ArrayReference.Length)
    pub async fn get_array_length(&mut self, array_id: ArrayId) -> JdwpResult<i32> {
        let id = self.next_id();
        let mut packet = CommandPacket::new(
            id,
            command_sets::ARRAY_REFERENCE,
            array_reference_commands::LENGTH,
        );
        packet.data.put_u64(array_id);

        let reply = self.send_command(packet).await?;
        reply.check_error()?;

        let mut data = reply.data();
        read_i32(&mut data)
    }

    /// Get array values (ArrayReference.GetValues)
    /// Returns tag + values. For object arrays, values are objectIDs.
    pub async fn get_array_values(
        &mut self,
        array_id: ArrayId,
        first_index: i32,
        length: i32,
    ) -> JdwpResult<Vec<Value>> {
        let id = self.next_id();
        let mut packet = CommandPacket::new(
            id,
            command_sets::ARRAY_REFERENCE,
            array_reference_commands::GET_VALUES,
        );
        packet.data.put_u64(array_id);
        packet.data.put_i32(first_index);
        packet.data.put_i32(length);

        let reply = self.send_command(packet).await?;
        reply.check_error()?;

        let mut data = reply.data();

        // Response: tag (1 byte) + count (4 bytes) + values
        let tag = read_u8(&mut data)?;
        let count = read_i32(&mut data)?;

        let mut values = Vec::with_capacity((count as usize).min(256));
        for _ in 0..count {
            let val = match tag {
                // Primitive arrays: untagged values
                66 => Value {
                    tag,
                    data: ValueData::Byte(crate::reader::read_i8(&mut data)?),
                },
                67 => Value {
                    tag,
                    data: ValueData::Char(crate::reader::read_u16(&mut data)?),
                },
                68 => Value {
                    tag,
                    data: ValueData::Double(crate::reader::read_f64(&mut data)?),
                },
                70 => Value {
                    tag,
                    data: ValueData::Float(crate::reader::read_f32(&mut data)?),
                },
                73 => Value {
                    tag,
                    data: ValueData::Int(read_i32(&mut data)?),
                },
                74 => Value {
                    tag,
                    data: ValueData::Long(crate::reader::read_i64(&mut data)?),
                },
                83 => Value {
                    tag,
                    data: ValueData::Short(crate::reader::read_i16(&mut data)?),
                },
                90 => Value {
                    tag,
                    data: ValueData::Boolean(read_u8(&mut data)? != 0),
                },
                // Object arrays: each element is tagged (tag + objectID)
                76 | 91 | 115 | 116 | 103 | 108 | 99 => {
                    let elem_tag = read_u8(&mut data)?;
                    let oid = crate::reader::read_u64(&mut data)?;
                    Value {
                        tag: elem_tag,
                        data: ValueData::Object(oid),
                    }
                }
                _ => Value {
                    tag: 86,
                    data: ValueData::Void,
                },
            };
            values.push(val);
        }

        Ok(values)
    }
}
