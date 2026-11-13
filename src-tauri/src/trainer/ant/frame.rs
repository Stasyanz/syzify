//! ANT serial messages: `A4 len id data… checksum`, the checksum being the
//! XOR of everything before it. USB reads cut the byte stream anywhere.

pub const SYNC: u8 = 0xA4;
/// Longest data part an ANT message has (extended burst data is ~30 bytes).
const MAX_DATA: usize = 40;

// Message ids (ANT Message Protocol and Usage, section 9).
pub const MSG_VERSION: u8 = 0x3E;
pub const MSG_CHANNEL_EVENT: u8 = 0x40;
pub const MSG_UNASSIGN: u8 = 0x41;
pub const MSG_ASSIGN: u8 = 0x42;
pub const MSG_PERIOD: u8 = 0x43;
pub const MSG_SEARCH_TIMEOUT: u8 = 0x44;
pub const MSG_RF_FREQ: u8 = 0x45;
pub const MSG_SET_KEY: u8 = 0x46;
pub const MSG_RESET: u8 = 0x4A;
pub const MSG_OPEN: u8 = 0x4B;
pub const MSG_CLOSE: u8 = 0x4C;
pub const MSG_BROADCAST: u8 = 0x4E;
pub const MSG_ACKNOWLEDGED: u8 = 0x4F;
pub const MSG_BURST: u8 = 0x50;
pub const MSG_CHANNEL_ID: u8 = 0x51;
pub const MSG_OPEN_SCAN: u8 = 0x5B;
pub const MSG_LOW_PRIORITY_SEARCH: u8 = 0x63;
pub const MSG_LIB_CONFIG: u8 = 0x6E;
pub const MSG_STARTUP: u8 = 0x6F;

/// In a channel response, this "message id" marks an event rather than an
/// answer to a command.
pub const CHANNEL_EVENT: u8 = 0x01;
pub const EVENT_RX_FAIL: u8 = 0x02;
pub const EVENT_TRANSFER_TX_COMPLETED: u8 = 0x05;
pub const EVENT_TRANSFER_TX_FAILED: u8 = 0x06;
pub const EVENT_CHANNEL_CLOSED: u8 = 0x07;
pub const EVENT_RX_FAIL_GO_TO_SEARCH: u8 = 0x08;

/// Lib config: append the sender's channel id to every data message.
pub const LIB_CHANNEL_ID: u8 = 0x80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msg {
    pub id: u8,
    pub data: Vec<u8>,
}

impl Msg {
    pub fn new(id: u8, data: &[u8]) -> Msg {
        Msg { id, data: data.to_vec() }
    }

    /// The message as it goes over the wire.
    pub fn frame(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.data.len() + 4);
        out.push(SYNC);
        out.push(self.data.len() as u8);
        out.push(self.id);
        out.extend_from_slice(&self.data);
        out.push(out.iter().fold(0, |a, b| a ^ b));
        out
    }
}

/// Messages out of the byte stream: a read may hold several, or half of one.
#[derive(Debug, Default)]
pub struct Deframer {
    buf: Vec<u8>,
}

impl Deframer {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Msg> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            match self.buf.iter().position(|&b| b == SYNC) {
                Some(i) => {
                    self.buf.drain(..i);
                }
                None => {
                    self.buf.clear();
                    return out;
                }
            }
            if self.buf.len() < 2 {
                return out;
            }
            // Longer than any message: a stray 0xA4, not a start.
            if self.buf[1] as usize > MAX_DATA {
                self.buf.drain(..1);
                continue;
            }
            let total = self.buf[1] as usize + 4;
            if self.buf.len() < total {
                return out;
            }
            let sum = self.buf[..total - 1].iter().fold(0, |a, b| a ^ b);
            if sum == self.buf[total - 1] {
                out.push(Msg { id: self.buf[2], data: self.buf[3..total - 1].to_vec() });
                self.buf.drain(..total);
            } else {
                // Not a message start after all: skip this sync byte.
                self.buf.drain(..1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_frames_with_its_checksum() {
        let reset = Msg::new(MSG_RESET, &[0]);
        assert_eq!(reset.frame(), vec![0xA4, 0x01, 0x4A, 0x00, 0xA4 ^ 0x01 ^ 0x4A]);
    }

    #[test]
    fn messages_are_read_back_however_the_stream_is_cut() {
        let a = Msg::new(MSG_BROADCAST, &[0, 0x19, 1, 2, 3, 4, 5, 6, 7]);
        let b = Msg::new(MSG_CHANNEL_EVENT, &[0, CHANNEL_EVENT, EVENT_RX_FAIL]);
        let stream: Vec<u8> = [vec![0x00, 0x13], a.frame(), b.frame()].concat();
        for cut in 0..stream.len() {
            let mut d = Deframer::default();
            let mut got = d.push(&stream[..cut]);
            got.extend(d.push(&stream[cut..]));
            assert_eq!(got, vec![a.clone(), b.clone()], "cut at {cut}");
        }
    }

    #[test]
    fn noise_and_a_bad_checksum_are_skipped() {
        let good = Msg::new(MSG_STARTUP, &[0x20]);
        let mut bad = good.frame();
        let last = bad.len() - 1;
        bad[last] ^= 0xFF;
        // A stray sync byte with an absurd length, a broken message, a good one.
        let stream = [vec![SYNC, 0xEE, 0x01], bad, good.frame()].concat();
        assert_eq!(Deframer::default().push(&stream), vec![good]);
        assert!(Deframer::default().push(&[1, 2, 3]).is_empty(), "no sync at all");
    }
}
