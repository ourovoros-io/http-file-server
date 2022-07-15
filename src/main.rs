use serde::Deserialize;
use std::{
    io::{Read, Write},
    net::TcpListener,
};

fn main() {
    let mut listener_and_address = None;

    for port in 1025..65535 {
        let address = format!("127.0.0.1:{port}");

        if let Ok(listener) = TcpListener::bind(address.clone()) {
            listener_and_address = Some((listener, format!("http://{address}")));
            break;
        }
    }

    let (listener, address) = listener_and_address.expect("ERROR: No server ports available");

    println!("Listening for clients at {address}");

    'clients: loop {
        let (mut client_stream, client_address) = match listener.accept() {
            Ok(x) => x,
            Err(e) => {
                eprintln!("ERROR: Failed to accept client connection: {e}");
                continue;
            }
        };

        let mut message: Vec<u8> = vec![];

        loop {
            let mut buffer = [0u8; 1024];

            let buffer_size = match client_stream.read(&mut buffer) {
                Ok(x) => x,
                Err(e) => {
                    eprintln!("ERROR: Failed to read request from client {client_address}: {e}");
                    continue 'clients;
                }
            };

            message.extend(&buffer[..buffer_size]);

            if buffer_size < buffer.len() {
                break;
            }
        }

        let mut request_headers = [httparse::EMPTY_HEADER; 64];
        let mut request = httparse::Request::new(&mut request_headers);

        let request_data_start = match request.parse(message.as_slice()) {
            Ok(status) => match status {
                httparse::Status::Complete(x) => x,
                httparse::Status::Partial => {
                    eprintln!("ERROR: Incomplete request received from client {client_address}");
                    continue;
                }
            },
            Err(e) => {
                eprintln!("ERROR: Failed to parse request from client {client_address}: {e}");
                continue;
            }
        };

        let request_data = match String::from_utf8((&message[request_data_start..]).to_owned()) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("ERROR: Failed to parse request from client {client_address}: {e}");
                continue;
            }
        };

        let method = match request.method {
            Some(x) => x,
            None => {
                eprintln!("ERROR: Unsupported request from client {client_address}: {request:#?}");
                continue;
            }
        };

        let mut response_data: Vec<u8>;

        match method {
            "GET" => {
                #[derive(Debug, Deserialize)]
                struct GetRequest {
                    path: String,
                }

                match serde_json::from_str::<'_, GetRequest>(request_data.as_str()) {
                    Ok(get_request) => match std::fs::read(get_request.path.clone()) {
                        Ok(file_data) => {
                            response_data = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                                file_data.len(),
                            )
                            .as_bytes()
                            .to_owned();

                            response_data.extend(file_data);
                        }

                        Err(e) => {
                            eprintln!(
                                "ERROR: Failed to handle {method} request from client {client_address}: {e}"
                            );
                            response_data = "HTTP/1.1 404 NOT FOUND\r\n".as_bytes().to_owned();
                        }
                    },

                    Err(e) => {
                        eprintln!("ERROR: Failed to deserialize {method} request from client {client_address}: {e}");
                        response_data = "HTTP/1.1 404 NOT FOUND\r\n".as_bytes().to_owned();
                    }
                }
            }

            "POST" => {
                #[derive(Debug, Deserialize)]
                struct PostRequest {
                    path: String,
                    data: Vec<u8>,
                }

                match serde_json::from_str::<'_, PostRequest>(request_data.as_str()) {
                    Ok(post_request) => if let Err(e) = std::fs::write(post_request.path, post_request.data.as_slice()) {
                        eprintln!("ERROR: Failed to write data for {method} request from client {client_address}: {e}");
                        response_data = "HTTP/1.1 404 NOT FOUND\r\n".as_bytes().to_owned();
                    } else {
                        response_data = "HTTP/1.1 200 OK\r\n".as_bytes().to_owned();
                    }

                    Err(e) => {
                        eprintln!("ERROR: Failed to deserialize {method} request from client {client_address}: {e}");
                        response_data = "HTTP/1.1 404 NOT FOUND\r\n".as_bytes().to_owned();
                    }
                }
            }

            _ => {
                eprintln!("ERROR: Unsupported request from client {client_address}: {request:#?}");
                response_data = "HTTP/1.1 404 NOT FOUND\r\n".as_bytes().to_owned();
            }
        }

        if let Err(e) = client_stream.write(response_data.as_slice()) {
            eprintln!("ERROR: Failed to write {method} response for client {client_address}: {e}");
            continue;
        }

        if let Err(e) = client_stream.flush() {
            eprintln!("ERROR: Failed to flush {method} response for client {client_address}: {e}");
            continue;
        }

        println!("Handled {method} request for client {client_address}");
    }
}
