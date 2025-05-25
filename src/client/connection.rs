use std::net::{TcpListener, TcpStream};
use std::io::{BufReader, BufWriter, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task;
use super::cache_store::CacheStore;
use super::codec::RespCodec;
use super::model::RespValue;

#[derive(Debug, Clone)]
pub struct ReplicaConfig {
    pub master_host: Option<String>,
    pub master_port: Option<u16>,
    pub replica_host: String,
    pub replica_port: u16,
    pub role: String,
    pub master_replid: String,
    pub master_repl_offset: u64,
    pub connected_slaves: u32,
}

pub struct RedisServer {
    host: String,
    port: u16,
    data_store: Arc<Mutex<CacheStore>>,
    replica_config: Option<ReplicaConfig>,
}

impl RedisServer {
    pub fn new(host: String, port: u16, replica_config: Option<ReplicaConfig>) -> Self {
        RedisServer {
            host,
            port,
            data_store: Arc::new(Mutex::new(CacheStore::new())),
            replica_config,
        }
    }

    pub async fn run(&self) -> std::io::Result<()> {
        // If this is a replica, initiate handshake with master
        if let Some(ref config) = self.replica_config {
            if let (Some(master_host), Some(master_port)) = (&config.master_host, config.master_port) {
                println!("Connecting to master at {}:{}", master_host, master_port);
                if let Err(e) = self.initiate_replica_handshake(master_host, master_port).await {
                    eprintln!("Failed to connect to master: {}", e);
                    return Err(e);
                }
            }
        }

        let listener = TcpListener::bind(format!("{}:{}", self.host, self.port))?;
        println!("Listening on {}:{}", self.host, self.port);

        loop {
            let (stream, _) = listener.accept()?;
            let data_store = Arc::clone(&self.data_store);
            let replica_config = self.replica_config.clone();
            println!("Accepted connection");
            task::spawn(async move {
                if let Err(e) = handle_client(stream, data_store, replica_config).await {
                    eprintln!("Error handling client: {}", e);
                }
            });
        }
    }

    async fn initiate_replica_handshake(&self, master_host: &str, master_port: u16) -> std::io::Result<()> {
        // Connect to master
        let master_stream = TcpStream::connect(format!("{}:{}", master_host, master_port))?;
        let mut master_reader = BufReader::new(&master_stream);
        let mut master_writer = BufWriter::new(&master_stream);

        // Send PING command as RESP Array: *1\r\n$4\r\nPING\r\n
        let ping_command = RespValue::Array(vec![
            RespValue::BulkString("PING".to_string())
        ]);
        
        let encoded_ping = RespCodec::encode(&ping_command);
        master_writer.write_all(&encoded_ping)?;
        master_writer.flush()?;
        
        println!("Sent PING to master: {:?}", String::from_utf8_lossy(&encoded_ping));

        // Read response from master
        match RespCodec::decode(&mut master_reader) {
            Ok(response) => {
                println!("Received response from master: {:?}", response);
                // Expected response should be +PONG\r\n
                match response {
                    RespValue::SimpleString(s) if s == "PONG" => {
                        println!("Successfully received PONG from master");
                        Ok(())
                    }
                    _ => {
                        eprintln!("Unexpected response from master: {:?}", response);
                        Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unexpected response from master"))
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading response from master: {}", e);
                Err(e)
            }
        }
    }
}


async fn handle_client(stream: TcpStream, data_store: Arc<Mutex<CacheStore>>, replica_config: Option<ReplicaConfig>) -> std::io::Result<()> {
    let mut redis_reader = BufReader::new(&stream);
    let mut redis_writer = BufWriter::new(&stream);

    loop {
        match RespCodec::decode(&mut redis_reader) {
            Ok(RespValue::Array(commands)) => {
                println!("handle_client: redis_reader: {:?}\n commands: {:?} \n \n", redis_reader, commands);
                let response = process_command(commands, &data_store, &replica_config);
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
fn process_command(commands: Vec<RespValue>, data_store: &Arc<Mutex<CacheStore>>, replica_config: &Option<ReplicaConfig>) -> RespValue {
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
        "INFO" => {
            // TODO replica info 
            println!("\n\nreplica_config: {:?}\n\n", replica_config.clone());
            let role = if replica_config.is_some() {
                "role:slave"
            } else {
                "role:master"
            };
            
            // Add master replication ID and offset for master instances
            let info_response = if replica_config.is_none() {
                // This is a master instance
                format!("{}\r\nmaster_replid:8371b4fb1155b71f4a04d3e1bc3e18c4a990aeeb\r\nmaster_repl_offset:0", role)
            } else {
                // This is a slave instance, only return role
                role.to_string()
            };
            
            RespValue::BulkString(info_response)
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