use std::collections::HashMap;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

const LED_PATH: &str = "/sys/class/leds/led0/brightness";


#[tokio::main]
async fn main() {
    println!("Hello, world!");

    // Open tcp connection on localhost:8080
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("Failed to bind TCP listener");

    loop {
        // Accept incoming connections
        let (socket, _) = listener.accept().await.expect("Failed to accept connection");
        println!("Accepted connection from {:?}", socket);

        // Spawn a new task for each connection
        tokio::spawn(async move {
            // Handle the connection
            handle_connection(socket).await;
        });
    }
}

async fn handle_connection(mut socket: tokio::net::TcpStream) {
    // Buffer to read data
    let mut buffer = [0; 1024];

    loop {
        // Read data from the socket
        match socket.read(&mut buffer).await {
            Ok(0) => {
                // Connection closed
                println!("Connection closed");
                break;
            }
            Ok(n) => {
                let response = respond_to_request(&buffer[..n]).await;
                // Print the received data
                // println!("Received: {:?}", String::from_utf8_lossy(&buffer[..n]));

                socket
                    .write_all(&response)
                    .await
                    .expect("Failed to write response");
            }
            Err(e) => {
                // Handle error
                eprintln!("Error reading from socket: {:?}", e);
                break;
            }
        }
    }
}

async fn respond_to_request(request: &[u8]) -> Vec<u8> {
    let mut request = match Request::try_from(request) {
        Ok(request) => request,
        Err(e) => {
            eprintln!("Failed to parse request: {:?}", e);
            return Vec::new();
        }
    };

    if request.location.contains("?") {
        // Form data
        let location = request.location.clone();
        let parts: Vec<&str> = location.split('?').collect();
        request.location = parts[0].to_string();
        let query_string = parts[1];
        let query_params: Vec<&str> = query_string.split('&').collect();
        let mut param_map: HashMap<String, String> = HashMap::new();
        for param in query_params {
            let key_value: Vec<&str> = param.split('=').collect();
            if key_value.len() == 2 {
                let key = urlencoding::decode(key_value[0]).unwrap().to_string();
                let value = urlencoding::decode(key_value[1]).unwrap().to_string();
                param_map.insert(key, value);
            }
        }

        println!("Query Parameters: {:?}", param_map);

        // Set LED brightness from the query parameters
        if let Some(brightness) = param_map.get("brightness") {
            println!("Setting LED brightness to: {:?}", brightness);
            if let Ok(value) = brightness.parse::<u8>() {
                println!("Parsed brightness value: {:?}", value);
                println!("Writing to LED brightness file: {:?}", LED_PATH);
                let mut file = tokio::fs::OpenOptions::new()
                    .write(true)
                    .open(LED_PATH)
                    .await
                    .expect("Failed to open LED brightness file");
                file.write_all(brightness.as_bytes()).await.expect("Failed to write to LED brightness file");
                println!("LED brightness set to: {:?}", value);
            }
        }
    }

    if request.location == "/" {
        request.location = "/index.html".to_string();
    }

    println!("Request: {:?}", request.location);

    let pathbuf = std::path::PathBuf::from("./dist");
    let path = pathbuf.join(&request.location[1..]);

    println!("Path: {:?}", path);

    let file = tokio::fs::File::open(&path).await;
    if let Ok(mut file) = file {
        let mut contents = Vec::new();
        if let Err(e) = file.read_to_end(&mut contents).await {
            eprintln!("Failed to read file: {:?}", e);
            return Vec::new();
        }

        let mime_type = match path.extension() {
            Some(ext) => match ext.to_str() {
                Some("html") => "text/html",
                Some("css") => "text/css",
                Some("js") => "application/javascript",
                Some("png") => "image/png",
                Some("jpg") => "image/jpeg",
                Some("gif") => "image/gif",
                _ => "application/octet-stream",
            },
            None => "application/octet-stream",
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/html".to_string());
        headers.insert("Content-Length".to_string(), contents.len().to_string());
        headers.insert("Connection".to_string(), "close".to_string());

        let response = Response {
            status_code: 200,
            headers,
            body: contents,
        };

        return response.to_data();
    }



    
    
    let response = Response::not_found().await;

    // let response_string = response.to_string();
    // println!("Response: {:?}", response_string);
    // let response_bytes = response_string.as_bytes().to_vec();
    response.to_data()
}

struct Request {
    method: String,
    location: String,
    headers: HashMap<String, String>,
    body: String,
}

impl TryFrom<&[u8]> for Request {
    type Error = std::string::FromUtf8Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let request = String::from_utf8(value.to_vec())?;
        let request = snailquote::unescape(&request).unwrap_or_else(|_| request);

        let mut lines = request.lines();
        let start_line = lines.next().unwrap_or("");

        let mut parts = start_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let location = parts.next().unwrap_or("").to_string();

        let headers = lines
            .by_ref()
            .take_while(|line| !line.is_empty())
            .map(|line| {
                let mut parts = line.splitn(2, ':');
                let key = parts.next().unwrap_or("").trim();
                let value = parts.next().unwrap_or("").trim();
                (key.to_string(), value.to_string())
            })
            .collect::<HashMap<_, _>>();

        let body = lines.collect::<Vec<_>>().join("\n");

        Ok(Request {
            method,
            location,
            headers,
            body,
        })
    }
}

#[derive(Debug)]
struct Response {
    status_code: u16,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn status_code_to_string(status_code: u16) -> &'static str {
    match status_code {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown Status",
    }
}

impl Response {
    fn new(status_code: u16, body: Vec<u8>) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/plain".to_string());
        headers.insert("Content-Length".to_string(), body.len().to_string());

        Response {
            status_code,
            headers,
            body,
        }
    }

    fn to_data(&self) -> Vec<u8> {
        let status_line = format!("HTTP/1.1 {} {}\r\n", self.status_code, status_code_to_string(self.status_code));
        let headers = self.headers.iter()
            .map(|(k, v)| format!("{}: {}\r\n", k, v))
            .collect::<String>();
        let response = format!("{}{}\r\n", status_line, headers);
        let response = response.as_bytes().to_vec();
        let response = [response, self.body.clone()].concat();
        response
    }

    async fn not_found() -> Self {
        let body = tokio::fs::read("not_found.html")
            .await
            .unwrap_or_else(|_| b"<h1>404 Not Found</h1>".to_vec());

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/html".to_string());
        headers.insert("Content-Length".to_string(), body.len().to_string());
        headers.insert("Connection".to_string(), "close".to_string());

        Response {
            status_code: 404,
            headers,
            body,
        }
    }
}