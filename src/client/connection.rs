use std::net::{TcpListener, TcpStream};
use std::io::{BufReader, BufWriter, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task;
use super::cache_store::CacheStore;
use super::codec::RespCodec;
use super::model::RespValue;

pub struct RedisServer {
    host: String,
    port: u16,
    data_store: Arc<Mutex<CacheStore>>,
}

impl RedisServer {
    pub fn new(host: String, port: u16) -> Self {
        RedisServer {
            host,
            port,
            data_store: Arc::new(Mutex::new(CacheStore::new())),
        }
    }

    pub async fn run(&self) -> std::io::Result<()> {
        let listener = TcpListener::bind(format!("{}:{}", self.host, self.port))?;
        println!("Listening on {}:{}", self.host, self.port);

        loop {
            let (stream, _) = listener.accept()?;
            let data_store = Arc::clone(&self.data_store);
            println!("Accepted connection");
            task::spawn(async move {
                if let Err(e) = handle_client(stream, data_store).await {
                    eprintln!("Error handling client: {}", e);
                }
            });
        }
    }
}


async fn handle_client(stream: TcpStream, data_store: Arc<Mutex<CacheStore>>) -> std::io::Result<()> {
    let mut redis_reader = BufReader::new(&stream);
    let mut redis_writer = BufWriter::new(&stream);

    loop {
        match RespCodec::decode(&mut redis_reader) {
            Ok(RespValue::Array(commands)) => {
                println!("handle_client: redis_reader: {:?}\n commands: {:?} \n \n", redis_reader, commands);
                let response = process_command(commands, &data_store);
                redis_writer.write_all(&RespCodec::encode(&response))?;
                redis_writer.flush()?;
                println!("handle_client: response: {:?} \n \n", response);
            }
            Ok(other) => {
                eprintln!("handle_client: Unexpected data type: {:?}", other);
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Expected array"));
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                eprintln!("handle_client: Unexpected EOF: {}", e);
                break;
            }
            Err(e) => {
                eprintln!("handle_client: Error decoding command: {}", e);
                return Err(e);
            }
        }
    }

    Ok(())
}
fn process_command(commands: Vec<RespValue>, data_store: &Arc<Mutex<CacheStore>>) -> RespValue {
    if commands.is_empty() {
        return RespValue::Error("ERR no command specified".to_string());
    }


    let command = match &commands[0] {
        RespValue::BulkString(s) | RespValue::SimpleString(s) => s.to_uppercase(),
        RespValue::BinaryBulkString(b) => {
            match String::from_utf8(b.clone()) {
                Ok(s) => s.to_uppercase(),
                Err(_) => return RespValue::Error("ERR invalid command: non-UTF8 data".to_string()),
            }
        },
        _ => return RespValue::Error("ERR invalid command: expected string".to_string()),
    };

    
    // let args: Option<String> = match &commands[3] {
    //     RespValue::BulkString(s) | RespValue::SimpleString(s) => Some(s.to_uppercase()),
    //     RespValue::BinaryBulkString(b) => {
    //         match String::from_utf8(b.clone()) {
    //             Ok(s) => Some(s.to_uppercase()),
    //             Err(_) => return RespValue::Error("ERR invalid command: non-UTF8 data".to_string()),
    //         }
    //     },
    //     _ => None,
    // };

    match command.as_str() {
        "PING" => RespValue::SimpleString("PONG".to_string()),
        "SET" => {
            if commands.len() < 3 {
                return RespValue::Error("ERR wrong number of arguments for 'set' command: expected 3".to_string());
            }
            let mut store = data_store.lock().unwrap();
            let key = match &commands[1] {
                RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
                RespValue::BinaryBulkString(b) => match String::from_utf8(b.clone()) {
                    Ok(s) => s,
                    Err(_) => return RespValue::Error("ERR invalid key: non-UTF8 data".to_string()),
                },
                _ => return RespValue::Error("ERR invalid key: expected string".to_string()),
            };
            let value = match &commands[2] {
                RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
                RespValue::BinaryBulkString(b) => match String::from_utf8(b.clone()) {
                    Ok(s) => s,
                    Err(_) => return RespValue::Error("ERR invalid value: non-UTF8 data".to_string()),
                },
                _ => return RespValue::Error("ERR invalid value: expected string".to_string()),
            };
            
            let expiry = if commands.len() > 3 {
                parse_px(&commands[3..])
            } else {
                None
            };

            store.set(key, value, expiry);
            RespValue::SimpleString("OK".to_string())
        }
        "GET" => {
            if commands.len() < 2 {
                return RespValue::Error("ERR wrong number of arguments for 'get' command: expected 2".to_string());
            }

            let key = match &commands[1] {
                RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
                RespValue::BinaryBulkString(b) => match String::from_utf8(b.clone()) {
                    Ok(s) => s,
                    Err(_) => return RespValue::Error("ERR invalid key: non-UTF8 data".to_string()),
                },
                _ => return RespValue::Error("ERR invalid key: expected string".to_string()),
            };
            let store = data_store.lock().unwrap();
            println!("process_command: store: {:?}", store);
            match store.get(&key) {
                Some(value) => {
                    if value.is_empty() {
                        return RespValue::Null;
                    }
                    return RespValue::BulkString(value.clone());
                },
                None => RespValue::Null,
            }
        }
        "ECHO" => {
            if commands.len() < 2 {
                return RespValue::Error("ERR wrong number of arguments for 'echo' command: expected 2".to_string());
            }
            commands[1].clone()
        }
        _ => RespValue::Error(format!("ERR unknown command: {}", command)),
    }
}


fn parse_px(args: &[RespValue]) -> Option<Duration> {
    let px = match &args[0] {
        RespValue::BulkString(s) | RespValue::SimpleString(s) => s.to_uppercase(),
        RespValue::BinaryBulkString(b) => match String::from_utf8(b.clone()) {
            Ok(s) => s.to_string().to_uppercase(),
            Err(_) => return None,
        },
        _ => return None,
    };

    if px != "PX" {
        return None;
    }

    let ms = match &args[1] {
        RespValue::Integer(i) => *i as u64,
        RespValue::BulkString(s) | RespValue::SimpleString(s) => s.parse().ok()?,
        RespValue::BinaryBulkString(b) => match String::from_utf8(b.clone()) {
            Ok(s) => s.parse().ok()?,
            Err(_) => return None,
        },
        _ => return None,
    };

    println!("ms: {:?}", ms);


    Some(Duration::from_millis(ms))
}