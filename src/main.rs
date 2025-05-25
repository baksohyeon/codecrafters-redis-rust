mod client;

use std::env;

use client::connection::{RedisServer, ReplicaConfig};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    println!("Redis 서버 시작 중...");

    let args: Vec<String> = env::args().collect();
    
    // Parse port
    let mut port = 6379u16;
    let mut replica_config: Option<ReplicaConfig> = None;
    
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                if i + 1 < args.len() {
                    port = args[i + 1].parse::<u16>().unwrap_or(6379);
                    i += 2;
                } else {
                    eprintln!("Error: --port requires a value");
                    i += 1;
                }
            }
            "--replicaof" => {
                if i + 1 < args.len() {
                    let replica_str = &args[i + 1];
                    let parts: Vec<&str> = replica_str.split_whitespace().collect();
                    if parts.len() == 2 {
                        let master_host = parts[0].to_string();
                        let master_port = parts[1].parse::<u16>().unwrap_or(6379);
                        replica_config = Some(ReplicaConfig {
                            master_host,
                            master_port,
                        });
                    } else {
                        eprintln!("Error: --replicaof requires format 'host port'");
                    }
                    i += 2;
                } else {
                    eprintln!("Error: --replicaof requires a value");
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    println!("port: {}", port);
    println!("args: {:?}", args);
    if let Some(ref config) = replica_config {
        println!("Replica mode: connecting to master at {}:{}", config.master_host, config.master_port);
    }

    let server = RedisServer::new("127.0.0.1".to_string(), port, replica_config);
    server.run().await
}