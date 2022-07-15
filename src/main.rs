use serde::Deserialize;
use std::{
    error::Error,
    fs,
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream},
    thread,
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

    loop {
        let (client_stream, client_address) = match listener.accept() {
            Ok(x) => x,
            Err(e) => {
                eprintln!("ERROR: Failed to accept client connection: {e}");
                continue;
            }
        };

        thread::spawn(move || handle_connection(client_stream, client_address));
    }
}

fn read_message(client_stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut message: Vec<u8> = vec![];

    loop {
        let mut buffer = [0u8; 1024];

        let buffer_size = client_stream.read(&mut buffer)?;

        message.extend(&buffer[..buffer_size]);

        if buffer_size < buffer.len() {
            return Ok(message);
        }
    }
}

fn parse_request<'a, 'b>(
    request: &mut httparse::Request<'a, 'b>,
    message: &'b [u8],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let request_data_start = match request.parse(message)? {
        httparse::Status::Complete(x) => x,
        httparse::Status::Partial => return Err(Box::new(httparse::Error::Status)),
    };

    Ok((&message[request_data_start..]).to_owned())
}

fn handle_connection(mut client_stream: TcpStream, client_address: SocketAddr) {
    let is_localhost = match client_address.ip() {
        IpAddr::V4(ip) => ip == Ipv4Addr::LOCALHOST,
        IpAddr::V6(ip) => ip == Ipv6Addr::LOCALHOST,
    };

    if !is_localhost {
        eprintln!("ERROR: Non-local client {client_address} attempted connection");
        return;
    }

    let message = match read_message(&mut client_stream) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("ERROR: Failed to read request from client {client_address}: {e}");
            return;
        }
    };

    let mut request_headers = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut request_headers);

    let request_data = match parse_request(&mut request, &message) {
        Ok(x) => match String::from_utf8(x) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("ERROR: Failed to parse request from client {client_address}: {e} - {request:#?}");
                return;
            }
        },
        Err(e) => {
            eprintln!(
                "ERROR: Failed to parse request from client {client_address}: {e} - {request:#?}"
            );
            return;
        }
    };

    let request_method = match request.method {
        Some(x) => x,
        None => {
            eprintln!("ERROR: Unsupported request from client {client_address}: {request:#?}");
            return;
        }
    };

    let mut response_data: Vec<u8>;

    match request_method {
        "GET" => {
            #[derive(Debug, Deserialize)]
            struct GetRequest {
                path: String,
            }

            match serde_json::from_str::<'_, GetRequest>(request_data.as_str()) {
                Ok(get_request) => match fs::read(get_request.path.clone()) {
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
                            "ERROR: Failed to handle {request_method} request from client {client_address}: {e}"
                        );
                        response_data = "HTTP/1.1 404 NOT FOUND\r\n".as_bytes().to_owned();
                    }
                },

                Err(e) => {
                    eprintln!("ERROR: Failed to deserialize {request_method} request from client {client_address}: {e}");
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
                Ok(post_request) => {
                    if let Err(e) = fs::write(post_request.path, post_request.data.as_slice()) {
                        eprintln!("ERROR: Failed to write data for {request_method} request from client {client_address}: {e}");
                        response_data = "HTTP/1.1 404 NOT FOUND\r\n".as_bytes().to_owned();
                    } else {
                        response_data = "HTTP/1.1 200 OK\r\n".as_bytes().to_owned();
                    }
                }

                Err(e) => {
                    eprintln!("ERROR: Failed to deserialize {request_method} request from client {client_address}: {e}");
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
        eprintln!(
            "ERROR: Failed to write {request_method} response for client {client_address}: {e}"
        );
        return;
    }

    if let Err(e) = client_stream.flush() {
        eprintln!(
            "ERROR: Failed to flush {request_method} response for client {client_address}: {e}"
        );
        return;
    }

    println!("Handled {request_method} request for client {client_address}");
}
