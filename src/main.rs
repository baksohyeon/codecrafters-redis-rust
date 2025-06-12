mod client;

use std::env;

use client::connection::{RedisServer, ReplicaConfig};

#[tokio::main]
async fn main() -> std::io::Result<()> {

    let args: Vec<String> = env::args().collect();
    // Parse port
    let mut master_port = 6379u16;
    let mut replica_config: Option<ReplicaConfig> = None;
    let mut i = 1;
    while i < args.len() {
        println!("args[{}] = {:?}", i, args[i]);

        match args[i].as_str() {
            // master port
            "--port" => {
                if i + 1 < args.len() {
                    master_port = args[i + 1].parse::<u16>().unwrap_or(6379);
                    i += 2;
                } else {
                    eprintln!("Error: --port requires a value");
                    i += 1;
                }
            }
            // replication mode
            "--replicaof" => {
                if i + 1 < args.len() {
                    let replica_str = &args[i + 1];
                    let parts: Vec<&str> = replica_str.split_whitespace().collect();
                    if parts.len() == 2 {
                        let master_host = parts[0].to_string();
                        let master_port = parts[1].parse::<u16>().unwrap_or(6379);
                        replica_config = Some(ReplicaConfig {
                            master_host: Some(master_host),
                            master_port: Some(master_port),
                            replica_host: "127.0.0.1".to_string(),
                            replica_port: 0, // This will be set to the replica's own port later
                            role: "slave".to_string(),
                            master_replid: "".to_string(),
                            master_repl_offset: 0,
                            connected_slaves: 0,
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
                println!("Unknown argument: {}", args[i]);
                i += 1;
            }

        }
    }

    println!("port: {}", master_port);
    println!("args: {:?}", args);
    println!("replica_config: {:?}", replica_config.clone());


    // Update replica_port to match the actual port this replica is listening on
    let updated_replica_config = replica_config.map(|mut config| {
        config.replica_port = master_port;
        config
    });
    
    let server = RedisServer::new("127.0.0.1".to_string(), master_port, updated_replica_config);
    server.run().await
}
