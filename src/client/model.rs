// referred source code: https://github.com/redis/node-redis/blob/master/packages/client/lib/commands/index.ts#L9
// referred source code: https://github.dev/iorust/resp/tree/master/src

/// Represents a RESP value, see [Redis Protocol specification](http://redis.io/topics/protocol).
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum RespValue {
    SimpleString(String), // For Simple Strings the first byte of the reply is "+".
    // Integer(i64), // For Integers the first byte of the reply is ":".
    Integer(u64), // For Integers the first byte of the reply is ":".
    BinaryBulkString(Vec<u8>), // For Bulk <binary> Strings the first byte of the reply is "$".
    BulkString(String), // For Bulk Strings the first byte of the reply is "$".
    Error(String), // For Errors the first byte of the reply is "-".
    Null, // Null bulk reply, `$-1\r\n`
    NullArray, // Null array reply, `*-1\r\n`
    Array(Vec<RespValue>), // For Arrays the first byte of the reply is "*".
}