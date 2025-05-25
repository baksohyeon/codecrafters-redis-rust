// referred source code: https://github.com/redis/node-redis/blob/master/packages/client/lib/commands/index.ts#L9
// referred source code: https://github.dev/iorust/resp/tree/master/src

/// Represents a RESP value, see [Redis Protocol specification](http://redis.io/topics/protocol).
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum RespValue {
    SimpleString(String), // For Simple Strings the first byte of the reply is "+".
    Integer(u64), // For Integers the first byte of the reply is ":".
    BinaryBulkString(Vec<u8>), // For Bulk <binary> Strings the first byte of the reply is "$".
    BulkString(String), // For Bulk Strings the first byte of the reply is "$".
    Error(String), // For Errors the first byte of the reply is "-".
    Null, // Null bulk reply, `$-1\r\n`
    NullArray, // Null array reply, `*-1\r\n`
    Array(Vec<RespValue>), // For Arrays the first byte of the reply is "*".
}

impl RespValue {
    // pub fn is_null(&self) -> bool {
    //     match self {
    //         RespValue::Null => true,
    //         RespValue::NullArray => true,
    //         _ => false,
    //     }
    // }


    // pub fn is_error(&self) -> bool {
    //     matches!(self, RespValue::Error(_))
    // }

    // pub fn is_array(&self) -> bool {
    //     match self {
    //         RespValue::Array(_) => true,
    //         RespValue::NullArray => true,
    //         RespValue::BinaryBulkString(_) => true,
    //         _ => false,
    //     }
    // }

    // pub fn to_string(&self) -> String {
    //     match self {
    //         RespValue::SimpleString(s) => s.clone(),
    //         RespValue::BulkString(s) => s.clone(),
    //         RespValue::Array(s) => {
    //             let mut response: Vec<u8> = Vec::new();
    //             response.push(b'*');
    //             response.extend_from_slice(s.len().to_string().as_bytes());
    //             response.push(b'\r');
    //             response.push(b'\n');
    //             for item in s {
    //                 response.extend_from_slice(item.to_string().as_bytes());
    //             }
    //             String::from_utf8(response).unwrap_or_default()
    //         }
    //         _ => "".to_string(),
    //     }
    // }
}