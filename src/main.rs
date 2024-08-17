mod client;

use std::env;

use client::connection::RedisServer;




#[tokio::main]
async fn main() -> std::io::Result<()> {
    println!("Redis 서버 시작 중...");

    let args: Vec<String> = env::args().collect();
    let port = args
        .get(2)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(6379);

    println!("port: {}", port);
    println!("args: {:?}", args);

    let server = RedisServer::new("127.0.0.1".to_string(), port);
    server.run().await
}