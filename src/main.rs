use std::{io::Write, net::TcpListener};

fn main() {
    println!("Logs from your program will appear here!");

    
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();
    
    for stream in listener.incoming() {
        match stream {
            Ok(_stream) => {
                println!("accepted new connection");
                println!("{:?}", _stream);
                let mut tcp_stream = _stream;
                let response = "+PONG\r\n";
                tcp_stream.write(response.as_bytes()).expect("Failed to write to stream");
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}
