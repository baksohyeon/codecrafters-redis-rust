use std::{io::{Read, Write}, net::TcpListener};

fn main() {
	println!("Logs from your program will appear here!");

	
	let listener = TcpListener::bind("127.0.0.1:6379").expect("Failed to bind to port 6379");
	
	for stream in listener.incoming() {
		match stream {
			Ok(stream) => {
				println!("accepted new connection");
				println!("{:?}", stream);
				let mut tcp_stream = stream;
				let response = "+PONG\r\n";

				let mut buffer = [0; 1024];

				loop {
					match tcp_stream.read(&mut buffer) {
						Ok(0) => break,
						Ok(_) => {
							tcp_stream.write(response.as_bytes()).expect("Failed to write to stream");
						}
						Err(e) => {
							println!("multiple Ping response error: {}", e);
							break;
						}
					}
				}

				println!("read from stream: {}", String::from_utf8_lossy(&buffer));
			}
			Err(e) => {
				println!("error: {}", e);
			}
		}
	}
}