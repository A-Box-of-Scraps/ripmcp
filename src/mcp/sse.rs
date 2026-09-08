use super::{limit_error, protocol, protocol_error};
use crate::error::Error;
use serde_json::Value;

pub(super) struct Events {
    line: Vec<u8>,
    data: Vec<u8>,
    event: Vec<u8>,
    bytes: usize,
    limit: usize,
    after_cr: bool,
    first_line: bool,
}

impl Events {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            line: Vec::new(),
            data: Vec::new(),
            event: Vec::new(),
            bytes: 0,
            limit,
            after_cr: false,
            first_line: true,
        }
    }

    pub(super) fn feed(&mut self, chunk: &[u8], id: &str) -> Result<Option<Value>, Error> {
        for &byte in chunk {
            if self.after_cr && byte == b'\n' {
                self.after_cr = false;
                continue;
            }
            self.after_cr = byte == b'\r';
            self.bytes = self.bytes.checked_add(1).ok_or_else(limit_error)?;
            if self.bytes > self.limit {
                return Err(limit_error());
            }
            if byte != b'\r' && byte != b'\n' {
                self.line.push(byte);
                continue;
            }
            if let Some(result) = self.line(id)? {
                return Ok(Some(result));
            }
        }
        Ok(None)
    }

    fn line(&mut self, id: &str) -> Result<Option<Value>, Error> {
        let mut line: Vec<u8> = std::mem::take(&mut self.line);
        if self.first_line && line.starts_with(&[0xef, 0xbb, 0xbf]) {
            line.drain(..3);
        }
        self.first_line = false;
        std::str::from_utf8(&line).map_err(|_| protocol_error())?;
        if line.is_empty() {
            return self.dispatch(id);
        }
        let (field, value): (&[u8], &[u8]) = match line.iter().position(|byte| *byte == b':') {
            Some(position) => (&line[..position], &line[position + 1..]),
            None => (&line, &[]),
        };
        let value: &[u8] = value.strip_prefix(b" ").unwrap_or(value);
        match field {
            b"data" => {
                self.data.extend_from_slice(value);
                self.data.push(b'\n');
            }
            b"event" => self.event = value.to_vec(),
            _ => (),
        }
        Ok(None)
    }

    fn dispatch(&mut self, id: &str) -> Result<Option<Value>, Error> {
        self.bytes = 0;
        let mut data: Vec<u8> = std::mem::take(&mut self.data);
        let event: Vec<u8> = std::mem::take(&mut self.event);
        if data.is_empty() {
            return Ok(None);
        }
        if !event.is_empty() && event != b"message" {
            return Err(protocol_error());
        }
        data.pop();
        match protocol::decode(&data)? {
            protocol::Message::Notification => Ok(None),
            protocol::Message::Response {
                id: response_id,
                result,
            } => {
                if response_id.as_deref() != Some(id) {
                    return Err(protocol_error());
                }
                result.map(Some)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Events;
    use crate::error::ErrorKind;
    use serde_json::{Value, json};

    #[test]
    fn arbitrarily_fragmented_events_support_all_line_endings() {
        for ending in ["\n", "\r", "\r\n"] {
            let response: Value = json!({"jsonrpc": "2.0", "id": "ripmcp-1", "result": {"resultType": "complete", "content": [], "extra": "\u{2603}"}});
            let bytes: Vec<u8> = format!("\u{feff}: heartbeat{ending}{ending}event: message{ending}id: ignored{ending}data: {response}{ending}{ending}").into_bytes();
            for size in [1, 2, 7, 127] {
                assert_eq!(fragmented(&bytes, size), response["result"]);
            }
        }
    }

    fn fragmented(bytes: &[u8], size: usize) -> Value {
        let mut events: Events = Events::new(4096);
        let mut result: Option<Value> = None;
        for chunk in bytes.chunks(size) {
            result = events.feed(chunk, "ripmcp-1").unwrap().or(result);
        }
        result.unwrap()
    }

    #[test]
    fn malformed_and_oversized_streams_fail_explicitly() {
        for bytes in [
            b"data: not-json\n\n".as_slice(),
            b"event: endpoint\ndata: {}\n\n",
            b"data: {\"jsonrpc\":\"2.0\",\"id\":\"ripmcp-2\",\"result\":{}}\n\n",
            b"data: \xff\n\n",
        ] {
            assert_eq!(
                Events::new(4096).feed(bytes, "ripmcp-1").unwrap_err().kind,
                ErrorKind::Protocol
            );
        }
        assert_eq!(
            Events::new(16)
                .feed(&[b'x'; 17], "ripmcp-1")
                .unwrap_err()
                .kind,
            ErrorKind::Protocol
        );
    }
}
