use crate::{
    machine::Board,
    orbit::{self, Pass},
};
use std::collections::VecDeque;
pub fn crc16(bytes: &[u8]) -> u16 {
    let mut c = 0xffffu16;
    for b in bytes {
        c ^= (*b as u16) << 8;
        for _ in 0..8 {
            c = if c & 0x8000 != 0 {
                (c << 1) ^ 0x1021
            } else {
                c << 1
            };
        }
    }
    c
}
pub fn frame(seq: u16, payload: &[u8]) -> Vec<u8> {
    let mut f = vec![0xa7, 0x5c];
    f.extend(seq.to_le_bytes());
    f.extend((payload.len() as u16).to_le_bytes());
    f.extend(payload);
    f.extend(crc16(&f).to_le_bytes());
    f
}
pub fn unframe(bytes: &[u8]) -> Result<(u16, Vec<u8>), String> {
    if bytes.len() < 8 || bytes[..2] != [0xa7, 0x5c] {
        return Err("framing".into());
    }
    let n = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    if n == 0 || n > 257 || bytes.len() != n + 8 {
        return Err("length".into());
    }
    if crc16(&bytes[..bytes.len() - 2])
        != u16::from_le_bytes(bytes[bytes.len() - 2..].try_into().unwrap())
    {
        return Err("CRC".into());
    }
    Ok((
        u16::from_le_bytes([bytes[2], bytes[3]]),
        bytes[6..6 + n].to_vec(),
    ))
}
pub fn duration(bytes: usize, rate: u32) -> u64 {
    ((bytes as u64 + 16) * 10 * 1000).div_ceil(rate as u64)
}
#[derive(Clone, Debug, PartialEq)]
pub struct Air {
    pub end: u64,
    pub start: u64,
    pub wire: Vec<u8>,
    pub failed: bool,
    pub window_end: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub time: u64,
    pub direction: u8,
    pub state: String,
    pub wire: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Sim {
    pub board: Board,
    pub now: u64,
    pub pass: Pass,
    pub queue: VecDeque<Vec<u8>>,
    pub uplink: Option<Air>,
    pub downlink: Option<Air>,
    pub next_seq: u16,
    pub down_seq: u16,
    pub beacon_due: u64,
    pub rng: u64,
    pub records: Vec<Record>,
    pub received: Vec<(u64, u16, Vec<u8>)>,
    pub rate: u32,
    pub clean: bool,
}
impl Sim {
    pub fn new(board: Board) -> Self {
        let now = board.now;
        Self {
            now: board.now,
            pass: orbit::next_pass(board.now),
            board,
            queue: VecDeque::new(),
            uplink: None,
            downlink: None,
            next_seq: 1,
            down_seq: 1,
            beacon_due: ((now / 60000) + 1) * 60000,
            rng: 0x715109b1a1,
            records: vec![],
            received: vec![],
            rate: 1200,
            clean: false,
        }
    }
    pub fn queue(&mut self, p: Vec<u8>) -> u16 {
        let s = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        self.queue.push_back(frame(s, &p));
        s
    }
    fn record(&mut self, direction: u8, state: &str, wire: Vec<u8>) {
        self.records.push(Record {
            time: self.now,
            direction,
            state: state.into(),
            wire,
        });
        if self.records.len() > 4096 {
            self.records.drain(..1024);
        }
    }
    fn error(&mut self) -> bool {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        !self.clean
            && self.rng
                % if orbit::elevation(self.now) < 15.0 {
                    13
                } else {
                    127
                }
                == 0
    }
    pub fn contact(&self) -> bool {
        self.now >= self.pass.aos && self.now < self.pass.los
    }
    fn start(&mut self) {
        if self.contact() && self.uplink.is_none() {
            if let Some(wire) = self.queue.pop_front() {
                let end = self.now + duration(wire.len(), self.rate);
                let failed = self.error();
                self.record(0, "TX", wire.clone());
                self.uplink = Some(Air {
                    start: self.now,
                    end,
                    wire,
                    failed,
                    window_end: if self.contact() { self.pass.los } else { 0 },
                });
            }
        }
        if self.downlink.is_none() {
            if let Some(p) = self.board.tx.pop_front() {
                let wire = frame(self.down_seq, &p);
                self.down_seq = self.down_seq.wrapping_add(1);
                let end = self.now + duration(wire.len(), 2400);
                let failed = self.error();
                self.downlink = Some(Air {
                    start: self.now,
                    end,
                    wire,
                    failed,
                    window_end: if self.contact() { self.pass.los } else { 0 },
                });
            }
        }
    }
    pub fn advance(&mut self, ms: u64) {
        let end = self.now + ms;
        while self.now < end {
            if self.now >= self.pass.los {
                self.pass = orbit::next_pass(self.now + 1);
            }
            self.start();
            let mut next = end.min(self.beacon_due).min(self.pass.los);
            if self.now < self.pass.aos {
                next = next.min(self.pass.aos);
            }
            if let Some(a) = &self.uplink {
                next = next.min(a.end);
            }
            if let Some(a) = &self.downlink {
                next = next.min(a.end);
            }
            // An awake CPU advances at millisecond instruction budgets. Sleeping boards jump to their next device event.
            if !self.board.cpu.sleeping && !self.board.cpu.trapped {
                next = next.min(self.now + 1);
            }
            next = next.min(self.board.timer_due).min(self.board.watchdog);
            if let Some(op) = &self.board.flash_op {
                next = next.min(op.end);
            }
            if next <= self.now {
                next = self.now + 1;
            }
            self.board.advance(next - self.now, None);
            self.now = next;
            let lost = self.now >= self.pass.los;
            if self
                .uplink
                .as_ref()
                .is_some_and(|a| a.end <= self.now || lost)
            {
                let a = self.uplink.take().unwrap();
                if lost && a.end > self.pass.los {
                    self.record(0, "TRUNCATED", a.wire);
                } else {
                    if !a.failed {
                        if let Ok((_, p)) = unframe(&a.wire) {
                            self.board.receive(p);
                        }
                    }
                    self.record(0, "SENT", a.wire);
                }
            }
            if self.downlink.as_ref().is_some_and(|a| {
                a.window_end > 0 && self.now >= a.window_end && a.end > a.window_end
            }) {
                let a = self.downlink.as_mut().unwrap();
                let n = ((a.window_end - a.start) * 2400 / 10000).saturating_sub(16) as usize;
                let partial = a.wire[..n.min(a.wire.len())].to_vec();
                a.window_end = 0;
                self.record(1, "TRUNCATED", partial);
            }
            if self.downlink.as_ref().is_some_and(|a| a.end <= self.now) {
                let mut a = self.downlink.take().unwrap();
                if a.window_end > 0 && a.end <= a.window_end {
                    if a.failed && a.wire.len() > 8 {
                        a.wire[6] ^= 1;
                    }
                    match unframe(&a.wire) {
                        Ok((s, p)) => {
                            self.received.push((self.now, s, p));
                            if self.received.len() > 2048 {
                                self.received.drain(..512);
                            }
                            self.record(1, "RX", a.wire);
                        }
                        Err(_) => self.record(1, "BAD CRC", a.wire),
                    }
                }
            }
            if self.now >= self.beacon_due {
                let mut p = vec![0x80];
                p.extend(((self.now - self.board.boot_ms) / 1000).to_le_bytes()[..4].iter());
                p.extend(self.board.reset_count.to_le_bytes());
                if self.board.tx.len() < 32 {
                    self.board.tx.push_back(p);
                }
                self.beacon_due += 60000;
            }
        }
        if self.now >= self.pass.los {
            self.pass = orbit::next_pass(self.now + 1);
        }
    }
}
