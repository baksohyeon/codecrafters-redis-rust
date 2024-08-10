use std::io::{self, BufRead};
use super::model::RespValue;

pub struct RespCodec;

impl RespCodec {
    pub fn encode(value: &RespValue) -> Vec<u8> {
        match value {
            RespValue::SimpleString(s) => format!("+{}\r\n", s).into_bytes(),
            RespValue::Error(e) => format!("-{}\r\n", e).into_bytes(),
            RespValue::Integer(i) => format!(":{}\r\n", i).into_bytes(),
            RespValue::BulkString(s) => format!("${}\r\n{}\r\n", s.len(), s).into_bytes(),
            RespValue::BinaryBulkString(b) => {
                let mut result = format!("${}\r\n", b.len()).into_bytes();
                result.extend(b);
                result.extend(b"\r\n");
                result
            },
            RespValue::Array(arr) => {
                let mut result = format!("*{}\r\n", arr.len()).into_bytes();
                for item in arr {
                    result.extend(Self::encode(item));
                }
                result
            }
            RespValue::Null => "$-1\r\n".as_bytes().to_vec(),
            RespValue::NullArray => "*-1\r\n".as_bytes().to_vec(),
        }
    }

    pub fn decode<R: BufRead>(reader: &mut R) -> io::Result<RespValue> {
        let mut first_byte = [0u8; 1];
        reader.read_exact(&mut first_byte)?;

        match first_byte[0] {
            b'+' => {
                let mut buf = String::new();
                reader.read_line(&mut buf)?;
                Ok(RespValue::SimpleString(buf.trim_end().to_string()))
            }
            b'-' => {
                let mut buf = String::new();
                reader.read_line(&mut buf)?;
                Ok(RespValue::Error(buf.trim_end().to_string()))
            }
            b':' => {
                let mut buf = String::new();
                reader.read_line(&mut buf)?;
                let num = buf.trim_end().parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok(RespValue::Integer(num))
            }
            b'$' => {
                let mut buf = String::new();
                reader.read_line(&mut buf)?;
                let len: i64 = buf.trim_end().parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                if len == -1 {
                    return Ok(RespValue::Null);
                }
                let mut bulk = vec![0u8; len as usize];
                reader.read_exact(&mut bulk)?;
                reader.read_exact(&mut [0u8; 2])?; // Read and discard CRLF
                Ok(RespValue::BinaryBulkString(bulk))
            }
            b'*' => {
                let mut buf = String::new();
                reader.read_line(&mut buf)?;
                let len: i64 = buf.trim_end().parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                // println!("decode: len: {:?}, buf: {:?}", len, buf);
                if len == -1 {
                    return Ok(RespValue::NullArray);
                }
                let mut array = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    array.push(Self::decode(reader)?);
                    println!("decode: resp-value array: {:?}", array);
                }

                Ok(RespValue::Array(array))
            }
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid RESP data")),
        }
    }
}