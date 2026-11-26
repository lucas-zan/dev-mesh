//! Small RFC 6455 text transport for DevMesh's JSON stream. TLS stays in reqwest/proxy.
use crate::{Error, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX: usize = 1024 * 1024;
const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
fn io(error: std::io::Error) -> Error {
    Error::new("connection_error", error.to_string())
}
pub fn accept_key(key: &str) -> String {
    STANDARD.encode(
        ring::digest::digest(
            &ring::digest::SHA1_FOR_LEGACY_USE_ONLY,
            format!("{key}{GUID}").as_bytes(),
        )
        .as_ref(),
    )
}
pub struct Socket<S> {
    stream: S,
    server: bool,
    partial: Vec<u8>,
    fragmented: bool,
}
impl<S: AsyncRead + AsyncWrite + Unpin> Socket<S> {
    pub fn new(stream: S, server: bool) -> Self {
        Self {
            stream,
            server,
            partial: Vec::new(),
            fragmented: false,
        }
    }
    pub async fn send(&mut self, text: &str) -> Result<()> {
        write_frame(&mut self.stream, self.server, 1, text.as_bytes()).await
    }
    pub async fn receive(&mut self) -> Result<Option<String>> {
        loop {
            match read_frame(&mut self.stream, self.server).await? {
                None => return Ok(None),
                Some((_, 8, _)) => return Ok(None),
                Some((_, 9, payload)) => {
                    write_frame(&mut self.stream, self.server, 10, &payload).await?
                }
                Some((_, 10, _)) => {}
                Some((fin, opcode, payload)) => {
                    if let Some(text) = assemble(
                        &mut self.partial,
                        &mut self.fragmented,
                        fin,
                        opcode,
                        payload,
                    )? {
                        return Ok(Some(text));
                    }
                }
            }
        }
    }
    pub fn into_channels(
        self,
    ) -> (
        tokio::sync::mpsc::Sender<String>,
        tokio::sync::mpsc::Receiver<Result<String>>,
    )
    where
        S: Send + 'static,
    {
        let (mut read, mut write) = tokio::io::split(self.stream);
        let server = self.server;
        let (outgoing, mut out) = tokio::sync::mpsc::channel::<String>(64);
        let (control, mut controls) = tokio::sync::mpsc::channel::<Vec<u8>>(8);
        let (incoming, receive) = tokio::sync::mpsc::channel::<Result<String>>(64);
        tokio::spawn(async move {
            loop {
                let (opcode, payload) = tokio::select! {value=out.recv()=>match value{Some(v)=>(1,v.into_bytes()),None=>break},value=controls.recv()=>match value{Some(v)=>(10,v),None=>break}};
                if write_frame(&mut write, server, opcode, &payload)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
        tokio::spawn(async move {
            let mut partial = Vec::new();
            let mut fragmented = false;
            loop {
                let frame = match read_frame(&mut read, server).await {
                    Ok(Some(f)) => f,
                    Ok(None) => break,
                    Err(e) => {
                        let _ = incoming.send(Err(e)).await;
                        break;
                    }
                };
                match frame {
                    (_, 8, _) => break,
                    (_, 9, payload) => {
                        if control.send(payload).await.is_err() {
                            break;
                        }
                    }
                    (_, 10, _) => {}
                    (fin, opcode, payload) => {
                        match assemble(&mut partial, &mut fragmented, fin, opcode, payload) {
                            Ok(Some(text)) => {
                                if incoming.send(Ok(text)).await.is_err() {
                                    break;
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                let _ = incoming.send(Err(e)).await;
                                break;
                            }
                        }
                    }
                }
            }
        });
        (outgoing, receive)
    }
}
async fn write_frame<W: AsyncWrite + Unpin>(
    stream: &mut W,
    server: bool,
    opcode: u8,
    payload: &[u8],
) -> Result<()> {
    if payload.len() > MAX {
        return Err(Error::new(
            "payload_too_large",
            "WebSocket frame exceeds 1MiB",
        ));
    }
    let masked = !server;
    let flag = if masked { 128 } else { 0 };
    let mut bytes = vec![128 | opcode];
    match payload.len() {
        0..=125 => bytes.push(flag | payload.len() as u8),
        126..=65535 => {
            bytes.push(flag | 126);
            bytes.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        _ => {
            bytes.push(flag | 127);
            bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
    }
    if masked {
        let id = uuid::Uuid::new_v4();
        let mask = &id.as_bytes()[..4];
        bytes.extend_from_slice(mask);
        bytes.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    } else {
        bytes.extend_from_slice(payload);
    }
    stream.write_all(&bytes).await.map_err(io)?;
    stream.flush().await.map_err(io)
}
async fn read_frame<R: AsyncRead + Unpin>(
    read: &mut R,
    server: bool,
) -> Result<Option<(bool, u8, Vec<u8>)>> {
    let mut header = [0u8; 2];
    match read.read_exact(&mut header).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(io(e)),
    }
    let fin = header[0] & 128 != 0;
    let opcode = header[0] & 15;
    let masked = header[1] & 128 != 0;
    if header[0] & 112 != 0 || masked != server {
        return Err(Error::new(
            "protocol_error",
            "Invalid WebSocket flags or masking",
        ));
    }
    let length = match header[1] & 127 {
        126 => {
            let mut n = [0u8; 2];
            read.read_exact(&mut n).await.map_err(io)?;
            u16::from_be_bytes(n) as u64
        }
        127 => {
            let mut n = [0u8; 8];
            read.read_exact(&mut n).await.map_err(io)?;
            u64::from_be_bytes(n)
        }
        n => n as u64,
    };
    if length > MAX as u64 || (opcode >= 8 && (!fin || length > 125)) {
        return Err(Error::new(
            "payload_too_large",
            "Invalid WebSocket message size",
        ));
    }
    let mut mask = [0u8; 4];
    if masked {
        read.read_exact(&mut mask).await.map_err(io)?;
    }
    let mut payload = vec![0; length as usize];
    read.read_exact(&mut payload).await.map_err(io)?;
    if masked {
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[i % 4];
        }
    }
    Ok(Some((fin, opcode, payload)))
}
fn assemble(
    partial: &mut Vec<u8>,
    fragmented: &mut bool,
    fin: bool,
    opcode: u8,
    payload: Vec<u8>,
) -> Result<Option<String>> {
    match opcode {
        1 if !*fragmented => {
            *partial = payload;
            *fragmented = !fin;
        }
        0 if *fragmented => {
            partial.extend_from_slice(&payload);
            *fragmented = !fin;
        }
        _ => {
            return Err(Error::new(
                "protocol_error",
                "DevMesh expects text messages",
            ))
        }
    }
    if partial.len() > MAX {
        return Err(Error::new(
            "payload_too_large",
            "Fragmented message exceeds 1MiB",
        ));
    }
    if fin {
        return String::from_utf8(std::mem::take(partial))
            .map(Some)
            .map_err(|_| Error::new("protocol_error", "Message is not UTF-8"));
    }
    Ok(None)
}
pub async fn connect(endpoint: &str, key: &str) -> Result<Socket<reqwest::Upgraded>> {
    let url = endpoint
        .replacen("wss://", "https://", 1)
        .replacen("ws://", "http://", 1);
    let nonce = STANDARD.encode(uuid::Uuid::new_v4().as_bytes());
    let client = reqwest::Client::builder()
        .http1_only()
        .build()
        .map_err(|e| Error::new("connection_error", e.to_string()))?;
    let response = client
        .get(url)
        .bearer_auth(key)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", &nonce)
        .send()
        .await
        .map_err(|_| Error::new("connection_error", "Relay WebSocket handshake failed"))?;
    if response.status().as_u16() != 101
        || response
            .headers()
            .get("Sec-WebSocket-Accept")
            .and_then(|h| h.to_str().ok())
            != Some(accept_key(&nonce).as_str())
    {
        return Err(Error::new(
            "protocol_error",
            "Relay did not accept WebSocket handshake",
        ));
    }
    let stream = response
        .upgrade()
        .await
        .map_err(|e| Error::new("connection_error", e.to_string()))?;
    Ok(Socket::new(stream, false))
}
