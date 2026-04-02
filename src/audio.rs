use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub bytes: Vec<u8>,
    pub sent_at_ms: u64,
}

#[derive(Debug, Default)]
pub struct AdtsParser {
    buffer: Vec<u8>,
}

impl AdtsParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<AudioFrame> {
        self.buffer.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            let Some(start) = find_sync(&self.buffer) else {
                self.buffer.clear();
                break;
            };
            if start > 0 {
                self.buffer.drain(..start);
            }
            let Some(frame_len) = adts_frame_len(&self.buffer) else {
                break;
            };
            if self.buffer.len() < frame_len {
                break;
            }
            let frame = self.buffer.drain(..frame_len).collect();
            out.push(AudioFrame {
                bytes: frame,
                sent_at_ms: now_ms(),
            });
        }
        out
    }
}

fn find_sync(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(2)
        .position(|window| window[0] == 0xff && (window[1] & 0xf0) == 0xf0)
}

fn adts_frame_len(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 7 {
        return None;
    }
    if bytes[0] != 0xff || (bytes[1] & 0xf0) != 0xf0 {
        return None;
    }
    Some(
        (((bytes[3] & 0x03) as usize) << 11)
            | ((bytes[4] as usize) << 3)
            | (((bytes[5] & 0xe0) as usize) >> 5),
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::AdtsParser;

    #[test]
    fn parses_split_adts_frames() {
        let frame = [0xff, 0xf1, 0x50, 0x80, 0x01, 0x1f, 0xfc, 0x11];
        let mut parser = AdtsParser::new();
        assert!(parser.push(&frame[..4]).is_empty());
        let out = parser.push(&frame[4..]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].bytes, frame);
    }
}
