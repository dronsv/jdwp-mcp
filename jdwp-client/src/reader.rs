// Helper functions for reading JDWP data types from buffers

use crate::protocol::{JdwpError, JdwpResult};
use bytes::Buf;

/// Read a JDWP string (4-byte length prefix + UTF-8 bytes)
pub fn read_string(buf: &mut &[u8]) -> JdwpResult<String> {
    if buf.remaining() < 4 {
        return Err(JdwpError::Protocol(
            "Not enough data for string length".to_string(),
        ));
    }

    let len = buf.get_u32() as usize;

    if buf.remaining() < len {
        return Err(JdwpError::Protocol(format!(
            "Not enough data for string: expected {}, got {}",
            len,
            buf.remaining()
        )));
    }

    let bytes = &buf[..len];
    buf.advance(len);

    String::from_utf8(bytes.to_vec())
        .map_err(|e| JdwpError::Protocol(format!("Invalid UTF-8 in string: {}", e)))
}

/// Read a u32
pub fn read_u32(buf: &mut &[u8]) -> JdwpResult<u32> {
    if buf.remaining() < 4 {
        return Err(JdwpError::Protocol("Not enough data for u32".to_string()));
    }
    Ok(buf.get_u32())
}

/// Read a i32
pub fn read_i32(buf: &mut &[u8]) -> JdwpResult<i32> {
    if buf.remaining() < 4 {
        return Err(JdwpError::Protocol("Not enough data for i32".to_string()));
    }
    Ok(buf.get_i32())
}

/// Read a u8
pub fn read_u8(buf: &mut &[u8]) -> JdwpResult<u8> {
    if buf.remaining() < 1 {
        return Err(JdwpError::Protocol("Not enough data for u8".to_string()));
    }
    Ok(buf.get_u8())
}

/// Read a u64
pub fn read_u64(buf: &mut &[u8]) -> JdwpResult<u64> {
    if buf.remaining() < 8 {
        return Err(JdwpError::Protocol("Not enough data for u64".to_string()));
    }
    Ok(buf.get_u64())
}

/// Read an i8
pub fn read_i8(buf: &mut &[u8]) -> JdwpResult<i8> {
    if buf.remaining() < 1 {
        return Err(JdwpError::Protocol("Not enough data for i8".to_string()));
    }
    Ok(buf.get_i8())
}

/// Read a u16
pub fn read_u16(buf: &mut &[u8]) -> JdwpResult<u16> {
    if buf.remaining() < 2 {
        return Err(JdwpError::Protocol("Not enough data for u16".to_string()));
    }
    Ok(buf.get_u16())
}

/// Read an i16
pub fn read_i16(buf: &mut &[u8]) -> JdwpResult<i16> {
    if buf.remaining() < 2 {
        return Err(JdwpError::Protocol("Not enough data for i16".to_string()));
    }
    Ok(buf.get_i16())
}

/// Read an i32 (signed)
pub fn read_i64(buf: &mut &[u8]) -> JdwpResult<i64> {
    if buf.remaining() < 8 {
        return Err(JdwpError::Protocol("Not enough data for i64".to_string()));
    }
    Ok(buf.get_i64())
}

/// Read an f32
pub fn read_f32(buf: &mut &[u8]) -> JdwpResult<f32> {
    if buf.remaining() < 4 {
        return Err(JdwpError::Protocol("Not enough data for f32".to_string()));
    }
    Ok(buf.get_f32())
}

/// Read an f64
pub fn read_f64(buf: &mut &[u8]) -> JdwpResult<f64> {
    if buf.remaining() < 8 {
        return Err(JdwpError::Protocol("Not enough data for f64".to_string()));
    }
    Ok(buf.get_f64())
}

/// Read a tagged JDWP value based on its type tag (bounds-checked).
/// Shared by stackframe and object value reading.
pub fn read_value_by_tag(tag: u8, buf: &mut &[u8]) -> JdwpResult<crate::types::ValueData> {
    use crate::types::ValueData;
    match tag {
        66 => Ok(ValueData::Byte(read_i8(buf)?)),
        67 => Ok(ValueData::Char(read_u16(buf)?)),
        68 => Ok(ValueData::Double(read_f64(buf)?)),
        70 => Ok(ValueData::Float(read_f32(buf)?)),
        73 => Ok(ValueData::Int(read_i32(buf)?)),
        74 => Ok(ValueData::Long(read_i64(buf)?)),
        83 => Ok(ValueData::Short(read_i16(buf)?)),
        90 => Ok(ValueData::Boolean(read_u8(buf)? != 0)),
        86 => Ok(ValueData::Void),
        76 | 115 | 116 | 103 | 108 | 99 | 91 => {
            let object_id = read_u64(buf)?;
            Ok(ValueData::Object(object_id))
        }
        _ => Err(JdwpError::Protocol(format!("Unknown value tag: {}", tag))),
    }
}
