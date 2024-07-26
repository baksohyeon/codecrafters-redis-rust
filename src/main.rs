use std::sync::Arc;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    println!("Logs from your program will appear here!");

    let listener = TcpListener::bind("127.0.0.1:6379").expect("Failed to bind to port 6379");
    let db = Arc::new(Mutex::new(HashMap::new()));

    loop {
        let (socket, _) = listener.accept().unwrap();
        let db = Arc::clone(&db);
        tokio::spawn(async move { handle_connection_process(socket, db).await });
    }
}

pub async fn handle_connection_process(
    mut stream: TcpStream,
    db: Arc<Mutex<HashMap<String, String>>>,
) {
    println!("accepted new connection");
    let mut buffer = [0; 1024];

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(_) => {
                let request = String::from_utf8_lossy(&buffer);
                let response = process_request(&request, &db).await;
                stream
                    .write_all(response.as_bytes())
                    .expect("Failed to write to stream");
            }
            Err(e) => {
                println!("multiple Ping response error: {}", e);
                break;
            }
        }
    }

    println!("read from stream: {}", String::from_utf8_lossy(&buffer));
}

async fn process_request(request: &str, db: &Arc<Mutex<HashMap<String, String>>>) -> String {
    let parts: Vec<&str> = request.split("\r\n").collect();



    if parts.len() > 2 {
        match parts[2] {
            "SET" => {
                if parts.len() > 4 {
                    let key = parts[4].to_string();
                    let value = parts[6].to_string();
                    let mut db = db.lock().await;
                    db.insert(key, value);
                    return "+OK\r\n".to_string();
                }
            }
            "GET" => {
                if parts.len() > 4 {
                    let key = parts[4].to_string();
                    let db = db.lock().await;
                    if let Some(value) = db.get(&key) {
                        return format!("${}\r\n{}\r\n", value.len(), value);
                    } else {
                        return "$-1\r\n".to_string();
                    }
                }
            }
            "ECHO" => {
                return format!("${}\r\n{}\r\n", parts[4].len(), parts[4]);
            }
            _ => {
                return "+PONG\r\n".to_string();
            }
        }
    }
    "+PONG\r\n".to_string()
}
