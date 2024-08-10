mod client;

use client::connection::RedisServer;




#[tokio::main]
async fn main() -> std::io::Result<()> {
    println!("Redis 서버 시작 중...");

    let server = RedisServer::new("127.0.0.1".to_string(), 6379);
    server.run().await
}