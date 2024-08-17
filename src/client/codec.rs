use super::model::RespValue;
use std::io::{self, BufRead};

pub struct RespCodec;

impl RespCodec {
    // pub fn serialize_command(command: &str, args: &[&str]) -> Vec<u8> {
    //     let mut result = Vec::new();
        
    //     // 배열의 길이 (명령어 + 인자들)
    //     result.extend_from_slice(format!("*{}\r\n", args.len() + 1).as_bytes());
        
    //     // 명령어 직렬화
    //     result.extend_from_slice(format!("${}\r\n{}\r\n", command.len(), command).as_bytes());
        
    //     // 인자들 직렬화
    //     for arg in args {
    //         result.extend_from_slice(format!("${}\r\n{}\r\n", arg.len(), arg).as_bytes());
    //     }
        
    //     result
    // }

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
            }
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
                let num = buf
                    .trim_end()
                    .parse()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok(RespValue::Integer(num))
            }
            b'$' => {
                let mut buf = String::new();
                reader.read_line(&mut buf)?;
                let len: i64 = buf
                    .trim_end()
                    .parse()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                if len == -1 {
                    return Ok(RespValue::Null);
                }
                let mut bulk = vec![0u8; len as usize];
                let mut total_read = 0;
                while total_read < len as usize {
                    let read = reader.read(&mut bulk[total_read..])?;
                    if read == 0 {
                        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Unexpected EOF while reading bulk string"));
                    }
                    total_read += read;
                }
                reader.read_exact(&mut [0u8; 2])?; // Read and discard CRLF
                Ok(RespValue::BinaryBulkString(bulk))
            }
            b'*' => {
                let mut buf = String::new();
                reader.read_line(&mut buf)?;
                let len: i64 = buf.trim_end().parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                if buf.is_empty() {
                    return Ok(RespValue::Null);
                }
                if len == -1 {
                    return Ok(RespValue::NullArray);
                }



                let mut array = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    array.push(Self::decode(reader)?);
                }

                Ok(RespValue::Array(array))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid RESP data",
            )),
        }
    }
}
