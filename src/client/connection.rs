use super::cache_store::CacheStore;
use super::codec::RespCodec;
use super::model::RespValue;
use std::io::{BufReader, BufWriter, Write, BufRead, Read};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task;

// Empty RDB file content (hex decoded)
const EMPTY_RDB_FILE: &[u8] = &[
    0x52, 0x45, 0x44, 0x49, 0x53, 0x30, 0x30, 0x31, 0x31, 0xfa, 0x09, 0x72, 0x65, 0x64, 0x69, 0x73,
    0x2d, 0x76, 0x65, 0x72, 0x05, 0x37, 0x2e, 0x32, 0x2e, 0x30, 0xfa, 0x0a, 0x72, 0x65, 0x64, 0x69,
    0x73, 0x2d, 0x62, 0x69, 0x74, 0x73, 0xc0, 0x40, 0xfa, 0x05, 0x63, 0x74, 0x69, 0x6d, 0x65, 0xc2,
    0x6d, 0x08, 0xbc, 0x65, 0xfa, 0x08, 0x75, 0x73, 0x65, 0x64, 0x2d, 0x6d, 0x65, 0x6d, 0xc2, 0xb0,
    0xc4, 0x10, 0x00, 0xfa, 0x08, 0x61, 0x6f, 0x66, 0x2d, 0x62, 0x61, 0x73, 0x65, 0xc0, 0x00, 0xff,
    0xf0, 0x6e, 0x3b, 0xfe, 0xc0, 0xff, 0x5a, 0xa2
];

#[derive(Debug)]
enum CommandResponse {
    Normal(RespValue),
    PsyncWithRdb(RespValue), // FULLRESYNC response followed by RDB file
}

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

// Structure to hold replica connection information
#[derive(Debug)]
struct ReplicaConnection {
    writer: Arc<Mutex<BufWriter<TcpStream>>>,
}

impl ReplicaConnection {
    fn new(stream: TcpStream) -> Self {
        let writer = BufWriter::new(stream);
        ReplicaConnection {
            writer: Arc::new(Mutex::new(writer)),
        }
    }

    fn propagate_command(&self, command: &RespValue) -> std::io::Result<()> {
        let mut writer = self.writer.lock().unwrap();
        let encoded = RespCodec::encode(command);
        writer.write_all(&encoded)?;
        writer.flush()?;
        println!("Propagated command to replica: {:?}", command);
        Ok(())
    }
}

pub struct RedisServer {
    host: String,
    port: u16,
    data_store: Arc<Mutex<CacheStore>>,
    replica_config: Option<ReplicaConfig>,
    replica_connections: Arc<Mutex<Vec<ReplicaConnection>>>,
}

impl RedisServer {
    pub fn new(host: String, port: u16, replica_config: Option<ReplicaConfig>) -> Self {
        RedisServer {
            host,
            port,
            data_store: Arc::new(Mutex::new(CacheStore::new())),
            replica_config,
            replica_connections: Arc::new(Mutex::new(Vec::new())),
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
            let replica_connections = Arc::clone(&self.replica_connections);
            println!("Accepted connection");
            task::spawn(async move {
                if let Err(e) = handle_client(stream, data_store, replica_config, replica_connections).await {
                    eprintln!("Error handling client: {}", e);
                }
            });
        }
    }

    async fn initiate_replica_handshake(&self, master_host: &str, master_port: u16) -> std::io::Result<()> {
        // Connect to master
        let master_stream = TcpStream::connect(format!("{}:{}", master_host, master_port))?;
        let reader_stream = master_stream.try_clone()?;
        let writer_stream = master_stream.try_clone()?;
        let mut master_reader = BufReader::new(reader_stream);
        let mut master_writer = BufWriter::new(writer_stream);

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
                    }
                    _ => {
                        eprintln!("Unexpected response from master: {:?}", response);
                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unexpected response from master"));
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading response from master: {}", e);
                return Err(e);
            }
        }

        // Send first REPLCONF command: REPLCONF listening-port <PORT>
        let replconf_port_command = RespValue::Array(vec![
            RespValue::BulkString("REPLCONF".to_string()),
            RespValue::BulkString("listening-port".to_string()),
            RespValue::BulkString(self.port.to_string())
        ]);
        
        let encoded_replconf_port = RespCodec::encode(&replconf_port_command);
        master_writer.write_all(&encoded_replconf_port)?;
        master_writer.flush()?;
        
        println!("Sent REPLCONF listening-port to master: {:?}", String::from_utf8_lossy(&encoded_replconf_port));

        // Read response from master
        match RespCodec::decode(&mut master_reader) {
            Ok(response) => {
                println!("Received response from master: {:?}", response);
                // Expected response should be +OK\r\n
                match response {
                    RespValue::SimpleString(s) if s == "OK" => {
                        println!("Successfully received OK from master for REPLCONF listening-port");
                    }
                    _ => {
                        eprintln!("Unexpected response from master: {:?}", response);
                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unexpected response from master"));
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading response from master: {}", e);
                return Err(e);
            }
        }

        // Send second REPLCONF command: REPLCONF capa psync2
        let replconf_capa_command = RespValue::Array(vec![
            RespValue::BulkString("REPLCONF".to_string()),
            RespValue::BulkString("capa".to_string()),
            RespValue::BulkString("psync2".to_string())
        ]);
        
        let encoded_replconf_capa = RespCodec::encode(&replconf_capa_command);
        master_writer.write_all(&encoded_replconf_capa)?;
        master_writer.flush()?;
        
        println!("Sent REPLCONF capa psync2 to master: {:?}", String::from_utf8_lossy(&encoded_replconf_capa));

        // Read response from master
        match RespCodec::decode(&mut master_reader) {
            Ok(response) => {
                println!("Received response from master: {:?}", response);
                // Expected response should be +OK\r\n
                match response {
                    RespValue::SimpleString(s) if s == "OK" => {
                        println!("Successfully received OK from master for REPLCONF capa psync2");
                    }
                    _ => {
                        eprintln!("Unexpected response from master: {:?}", response);
                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unexpected response from master"));
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading response from master: {}", e);
                return Err(e);
            }
        }

        // Send PSYNC command: PSYNC ? -1
        let psync_command = RespValue::Array(vec![
            RespValue::BulkString("PSYNC".to_string()),
            RespValue::BulkString("?".to_string()),
            RespValue::BulkString("-1".to_string())
        ]);
        
        let encoded_psync = RespCodec::encode(&psync_command);
        master_writer.write_all(&encoded_psync)?;
        master_writer.flush()?;
        
        println!("Sent PSYNC ? -1 to master: {:?}", String::from_utf8_lossy(&encoded_psync));

        // Read response from master
        match RespCodec::decode(&mut master_reader) {
            Ok(response) => {
                println!("Received response from master: {:?}", response);
                // Expected response should be +FULLRESYNC <REPL_ID> 0\r\n
                match response {
                    RespValue::SimpleString(s) if s.starts_with("FULLRESYNC") => {
                        println!("Successfully received FULLRESYNC from master: {}", s);
                        
                        // After FULLRESYNC, we need to read the RDB file
                        // Read the RDB file header: $<length>\r\n
                        let mut rdb_header = String::new();
                        master_reader.read_line(&mut rdb_header)?;
                        
                        if rdb_header.starts_with('$') {
                            let rdb_length: usize = rdb_header[1..].trim().parse()
                                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                            
                            // Read the RDB file content
                            let mut rdb_content = vec![0u8; rdb_length];
                            master_reader.read_exact(&mut rdb_content)?;
                            
                            println!("Received RDB file from master ({} bytes)", rdb_length);
                            
                            // Now we're ready to receive commands from master
                            // Use the existing reader and writer for the listening task
                            let data_store = Arc::clone(&self.data_store);
                            task::spawn(async move {
                                if let Err(e) = listen_for_propagated_commands_with_streams(master_reader, master_writer, data_store).await {
                                    eprintln!("Error listening for propagated commands: {}", e);
                                }
                            });
                        }
                        
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

// Function that takes existing reader and writer streams
async fn listen_for_propagated_commands_with_streams(
    mut master_reader: BufReader<TcpStream>, 
    mut master_writer: BufWriter<TcpStream>, 
    data_store: Arc<Mutex<CacheStore>>
) -> std::io::Result<()> {
    let mut offset: u64 = 0; // Track bytes processed
    
    loop {
        match RespCodec::decode(&mut master_reader) {
            Ok(RespValue::Array(commands)) => {
                println!("Received propagated command from master: {:?}", commands);
                
                // Calculate the byte length of this command
                let command_bytes = RespCodec::encode(&RespValue::Array(commands.clone()));
                let command_byte_length = command_bytes.len() as u64;
                
                // Check if this is a REPLCONF GETACK command
                if let Some(first_command) = commands.get(0) {
                    let command = match first_command {
                        RespValue::BulkString(s) | RespValue::SimpleString(s) => s.to_uppercase(),
                        RespValue::BinaryBulkString(b) => {
                            match String::from_utf8(b.clone()) {
                                Ok(s) => s.to_uppercase(),
                                Err(_) => String::new(),
                            }
                        },
                        _ => String::new(),
                    };
                    
                    if command == "REPLCONF" && commands.len() >= 3 {
                        let subcommand = match &commands[1] {
                            RespValue::BulkString(s) | RespValue::SimpleString(s) => s.to_uppercase(),
                            RespValue::BinaryBulkString(b) => {
                                match String::from_utf8(b.clone()) {
                                    Ok(s) => s.to_uppercase(),
                                    Err(_) => String::new(),
                                }
                            },
                            _ => String::new(),
                        };
                        
                        if subcommand == "GETACK" {
                            // Respond with REPLCONF ACK <current_offset>
                            // The offset should be the bytes processed BEFORE this GETACK command
                            let ack_response = RespValue::Array(vec![
                                RespValue::BulkString("REPLCONF".to_string()),
                                RespValue::BulkString("ACK".to_string()),
                                RespValue::BulkString(offset.to_string())
                            ]);
                            
                            let encoded_response = RespCodec::encode(&ack_response);
                            master_writer.write_all(&encoded_response)?;
                            master_writer.flush()?;
                            println!("Sent REPLCONF ACK {} response to master", offset);
                            
                            // Now update the offset to include this GETACK command
                            offset += command_byte_length;
                            continue;
                        }
                    }
                    
                    // Handle PING command silently (no response to master)
                    if command == "PING" {
                        println!("Received PING from master, updating offset silently");
                        offset += command_byte_length;
                        continue;
                    }
                }
                
                // Process other commands and update offset
                process_command_for_replica(commands, &data_store);
                offset += command_byte_length;
            }
            Ok(other) => {
                println!("Received unexpected data from master: {:?}", other);
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                println!("Master connection closed");
                break;
            }
            Err(e) => {
                eprintln!("Error reading from master: {}", e);
                return Err(e);
            }
        }
    }
    Ok(())
}

// Move listen_for_propagated_commands outside the impl block and make it a standalone function
async fn listen_for_propagated_commands(master_stream: TcpStream, data_store: Arc<Mutex<CacheStore>>) -> std::io::Result<()> {
    let reader_stream = master_stream.try_clone()?;
    let writer_stream = master_stream;
    
    let master_reader = BufReader::new(reader_stream);
    let master_writer = BufWriter::new(writer_stream);
    
    listen_for_propagated_commands_with_streams(master_reader, master_writer, data_store).await
}

async fn handle_client(
    stream: TcpStream, 
    data_store: Arc<Mutex<CacheStore>>, 
    replica_config: Option<ReplicaConfig>,
    replica_connections: Arc<Mutex<Vec<ReplicaConnection>>>
) -> std::io::Result<()> {
    let stream_for_replica = stream.try_clone()?;
    let mut redis_reader = BufReader::new(&stream);
    let mut redis_writer = BufWriter::new(&stream);

    loop {
        match RespCodec::decode(&mut redis_reader) {
            Ok(RespValue::Array(commands)) => {
                println!("handle_client: commands: {:?}", commands);
                
                let response = process_command(commands.clone(), &data_store, &replica_config, &replica_connections);
                
                match response {
                    CommandResponse::Normal(resp_value) => {
                        redis_writer.write_all(&RespCodec::encode(&resp_value))?;
                        redis_writer.flush()?;
                        println!("handle_client: response: {:?}", resp_value);
                    }
                    CommandResponse::PsyncWithRdb(resp_value) => {
                        // First send the FULLRESYNC response
                        redis_writer.write_all(&RespCodec::encode(&resp_value))?;
                        redis_writer.flush()?;
                        println!("handle_client: FULLRESYNC response: {:?}", resp_value);
                        
                        // Then send the RDB file in the format: $<length>\r\n<binary_contents>
                        let rdb_header = format!("${}\r\n", EMPTY_RDB_FILE.len());
                        redis_writer.write_all(rdb_header.as_bytes())?;
                        redis_writer.write_all(EMPTY_RDB_FILE)?;
                        redis_writer.flush()?;
                        println!("handle_client: sent RDB file ({} bytes)", EMPTY_RDB_FILE.len());
                        
                        // Add this connection to replica connections for command propagation
                        let replica_conn = ReplicaConnection::new(stream_for_replica);
                        replica_connections.lock().unwrap().push(replica_conn);
                        println!("Added replica connection for command propagation");
                        
                        // Keep the connection alive for command propagation
                        // The replica will keep reading commands from this connection
                        return Ok(());
                    }
                }
                
                // If this is a write command and we're a master, propagate to replicas
                if replica_config.is_none() && is_write_command(&commands) {
                    propagate_to_replicas(&replica_connections, &RespValue::Array(commands));
                }
            }
            Ok(other) => {
                eprintln!("handle_client: Unexpected data type: {:?}", other);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Expected array",
                ));
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

fn is_write_command(commands: &[RespValue]) -> bool {
    if let Some(first_command) = commands.get(0) {
        let cmd = match first_command {
            RespValue::BulkString(s) | RespValue::SimpleString(s) => s.to_uppercase(),
            RespValue::BinaryBulkString(b) => {
                match String::from_utf8(b.clone()) {
                    Ok(s) => s.to_uppercase(),
                    Err(_) => return false,
                }
            },
            _ => return false,
        };
        
        match cmd.as_str() {
            "SET" | "DEL" | "INCR" | "DECR" | "LPUSH" | "RPUSH" | "LPOP" | "RPOP" => true,
            _ => false,
        }
    } else {
        false
    }
}

fn propagate_to_replicas(replica_connections: &Arc<Mutex<Vec<ReplicaConnection>>>, command: &RespValue) {
    let connections = replica_connections.lock().unwrap();
    for replica in connections.iter() {
        if let Err(e) = replica.propagate_command(command) {
            eprintln!("Failed to propagate command to replica: {}", e);
        }
    }
}

fn process_command(commands: Vec<RespValue>, data_store: &Arc<Mutex<CacheStore>>, replica_config: &Option<ReplicaConfig>, replica_connections: &Arc<Mutex<Vec<ReplicaConnection>>>) -> CommandResponse {
    if commands.is_empty() {
        return CommandResponse::Normal(RespValue::Error("ERR no command specified".to_string()));
    }

    let command = match &commands[0] {
        RespValue::BulkString(s) | RespValue::SimpleString(s) => s.to_uppercase(),
        RespValue::BinaryBulkString(b) => {
            match String::from_utf8(b.clone()) {
                Ok(s) => s.to_uppercase(),
                Err(_) => return CommandResponse::Normal(RespValue::Error("ERR invalid command: non-UTF8 data".to_string())),
            }
        },
        _ => return CommandResponse::Normal(RespValue::Error("ERR invalid command: expected string".to_string())),
    };

    match command.as_str() {
        "PING" => CommandResponse::Normal(RespValue::SimpleString("PONG".to_string())),
        "SET" => {
            if commands.len() < 3 {
                return CommandResponse::Normal(RespValue::Error("ERR wrong number of arguments for 'set' command: expected 3".to_string()));
            }
            let mut store = data_store.lock().unwrap();
            let key = match &commands[1] {
                RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
                RespValue::BinaryBulkString(b) => match String::from_utf8(b.clone()) {
                    Ok(s) => s,
                    Err(_) => return CommandResponse::Normal(RespValue::Error("ERR invalid key: non-UTF8 data".to_string())),
                },
                _ => return CommandResponse::Normal(RespValue::Error("ERR invalid key: expected string".to_string())),
            };
            let value = match &commands[2] {
                RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
                RespValue::BinaryBulkString(b) => match String::from_utf8(b.clone()) {
                    Ok(s) => s,
                    Err(_) => return CommandResponse::Normal(RespValue::Error("ERR invalid value: non-UTF8 data".to_string())),
                },
                _ => return CommandResponse::Normal(RespValue::Error("ERR invalid value: expected string".to_string())),
            };
            
            let expiry = if commands.len() > 3 {
                parse_px(&commands[3..])
            } else {
                None
            };

            store.set(key, value, expiry);
            CommandResponse::Normal(RespValue::SimpleString("OK".to_string()))
        }
        "GET" => {
            if commands.len() < 2 {
                return CommandResponse::Normal(RespValue::Error("ERR wrong number of arguments for 'get' command: expected 2".to_string()));
            }

            let key = match &commands[1] {
                RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
                RespValue::BinaryBulkString(b) => match String::from_utf8(b.clone()) {
                    Ok(s) => s,
                    Err(_) => return CommandResponse::Normal(RespValue::Error("ERR invalid key: non-UTF8 data".to_string())),
                },
                _ => return CommandResponse::Normal(RespValue::Error("ERR invalid key: expected string".to_string())),
            };
            let store = data_store.lock().unwrap();
            println!("process_command: store: {:?}", store);
            match store.get(&key) {
                Some(value) => {
                    if value.is_empty() {
                        return CommandResponse::Normal(RespValue::Null);
                    }
                    return CommandResponse::Normal(RespValue::BulkString(value.clone()));
                },
                None => CommandResponse::Normal(RespValue::Null),
            }
        }
        "ECHO" => {
            if commands.len() < 2 {
                return CommandResponse::Normal(RespValue::Error("ERR wrong number of arguments for 'echo' command: expected 2".to_string()));
            }
            CommandResponse::Normal(commands[1].clone())
        }
        "REPLCONF" => {
            // For the purposes of this challenge, we can safely ignore the arguments
            // and just respond with +OK\r\n ("OK" encoded as a RESP Simple String)
            CommandResponse::Normal(RespValue::SimpleString("OK".to_string()))
        }
        "PSYNC" => {
            // The master responds with +FULLRESYNC <REPL_ID> 0\r\n
            // FULLRESYNC means full resynchronization (not incremental)
            // <REPL_ID> is the replication ID of the master
            // 0 is the replication offset of the master
            let repl_id = "8371b4fb1155b71f4a04d3e1bc3e18c4a990aeeb";
            let response = format!("FULLRESYNC {} 0", repl_id);
            CommandResponse::PsyncWithRdb(RespValue::SimpleString(response))
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
            
            CommandResponse::Normal(RespValue::BulkString(info_response))
        }
        "WAIT" => {
            // WAIT numreplicas timeout
            // Return the actual number of connected replicas
            let replica_count = replica_connections.lock().unwrap().len() as u64;
            CommandResponse::Normal(RespValue::Integer(replica_count))
        }
        _ => CommandResponse::Normal(RespValue::Error(format!("ERR unknown command: {}", command))),
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

fn process_command_for_replica(commands: Vec<RespValue>, data_store: &Arc<Mutex<CacheStore>>) {
    if commands.is_empty() {
        return;
    }

    let command = match &commands[0] {
        RespValue::BulkString(s) | RespValue::SimpleString(s) => s.to_uppercase(),
        RespValue::BinaryBulkString(b) => {
            match String::from_utf8(b.clone()) {
                Ok(s) => s.to_uppercase(),
                Err(_) => return,
            }
        },
        _ => return,
    };

    match command.as_str() {
        "SET" => {
            if commands.len() < 3 {
                return;
            }
            let mut store = data_store.lock().unwrap();
            let key = match &commands[1] {
                RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
                RespValue::BinaryBulkString(b) => match String::from_utf8(b.clone()) {
                    Ok(s) => s,
                    Err(_) => return,
                },
                _ => return,
            };
            let value = match &commands[2] {
                RespValue::BulkString(s) | RespValue::SimpleString(s) => s.clone(),
                RespValue::BinaryBulkString(b) => match String::from_utf8(b.clone()) {
                    Ok(s) => s,
                    Err(_) => return,
                },
                _ => return,
            };
            
            let expiry = if commands.len() > 3 {
                parse_px(&commands[3..])
            } else {
                None
            };

            store.set(key, value, expiry);
            println!("Replica processed SET command");
        }
        _ => {
            // For other write commands, we would implement them here
            println!("Replica received command: {}", command);
        }
    }
}