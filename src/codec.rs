//! Versioned opaque containers. Obfuscation prevents accidental spoilers, not deliberate inspection.
use crate::{link::*, machine::*, orbit::Pass};
use std::collections::VecDeque;
pub struct Writer(pub Vec<u8>);
impl Writer {
    pub fn u8(&mut self, v: u8) {
        self.0.push(v)
    }
    pub fn bool(&mut self, v: bool) {
        self.u8(v as u8)
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend(v.to_le_bytes())
    }
    pub fn u64(&mut self, v: u64) {
        self.0.extend(v.to_le_bytes())
    }
    pub fn bytes(&mut self, v: &[u8]) {
        self.u32(v.len() as u32);
        self.0.extend(v)
    }
    pub fn string(&mut self, v: &str) {
        self.bytes(v.as_bytes())
    }
}
pub struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}
impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).ok_or("invalid container")?;
        let v = self.data.get(self.pos..end).ok_or("truncated container")?;
        self.pos = end;
        Ok(v)
    }
    pub fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    pub fn bool(&mut self) -> Result<bool, String> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err("invalid boolean".into()),
        }
    }
    pub fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn count(&mut self, max: usize) -> Result<usize, String> {
        let n = self.u32()? as usize;
        if n > max {
            Err("container limit exceeded".into())
        } else {
            Ok(n)
        }
    }
    pub fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let n = self.count(8 * 1024 * 1024)?;
        Ok(self.take(n)?.to_vec())
    }
    pub fn string(&mut self) -> Result<String, String> {
        String::from_utf8(self.bytes()?).map_err(|_| "invalid text".into())
    }
    pub fn done(&self) -> Result<(), String> {
        if self.pos == self.data.len() {
            Ok(())
        } else {
            Err("trailing container data".into())
        }
    }
}
fn checksum(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    })
}
fn mask(data: &mut [u8]) {
    let mut x = 0xa593b17c24ef069du64;
    for b in data {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b ^= (x >> 24) as u8;
    }
}
pub fn seal(data: &[u8]) -> Vec<u8> {
    let mut out = b"ORBIT001".to_vec();
    out.extend(checksum(data).to_le_bytes());
    let mut body = data.to_vec();
    mask(&mut body);
    out.extend(body);
    out
}
pub fn open(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 16 || data.len() > 64 * 1024 * 1024 || &data[..8] != b"ORBIT001" {
        return Err("unrecognized container".into());
    }
    let expected = u64::from_le_bytes(data[8..16].try_into().unwrap());
    let mut body = data[16..].to_vec();
    mask(&mut body);
    if checksum(&body) != expected {
        return Err("container checksum mismatch".into());
    }
    Ok(body)
}
fn registers(w: &mut Writer, r: &[u32; 8]) {
    for v in r {
        w.u32(*v)
    }
}
fn read_registers(r: &mut Reader) -> Result<[u32; 8], String> {
    let mut a = [0; 8];
    for v in &mut a {
        *v = r.u32()?;
    }
    Ok(a)
}
fn packets(w: &mut Writer, q: &VecDeque<Vec<u8>>) {
    w.u32(q.len() as u32);
    for p in q {
        w.bytes(p)
    }
}
fn read_packets(r: &mut Reader, max: usize) -> Result<VecDeque<Vec<u8>>, String> {
    let n = r.count(max)?;
    let mut q = VecDeque::new();
    for _ in 0..n {
        let p = r.bytes()?;
        if p.len() > 4096 {
            return Err("packet too long".into());
        }
        q.push_back(p);
    }
    Ok(q)
}
pub fn write_board(w: &mut Writer, b: &Board) {
    registers(w, &b.cpu.r);
    w.u32(b.cpu.pc);
    w.bool(b.cpu.zero);
    w.bool(b.cpu.carry);
    w.bool(b.cpu.sleeping);
    w.bool(b.cpu.trapped);
    w.bool(b.cpu.irq_enabled);
    w.u64(b.cpu.cycles);
    w.bool(b.cpu.shadow.is_some());
    if let Some((r, p, z, c)) = &b.cpu.shadow {
        registers(w, r);
        w.u32(*p);
        w.bool(*z);
        w.bool(*c);
    }
    w.bytes(&b.memory);
    w.u32(b.faults.len() as u32);
    for f in &b.faults {
        w.u32(f.start);
        w.u32(f.end);
        w.u32(f.address_and);
        w.u8(f.data_and);
        w.u8(f.data_or);
        w.bool(f.write_inhibit);
    }
    w.u64(b.now);
    w.u64(b.boot_ms);
    w.u32(b.reset_count);
    w.u32(b.reset_cause);
    w.u64(b.watchdog);
    w.u64(b.timer_due);
    w.bool(b.pending_irq);
    packets(w, &b.rx);
    w.u32(b.rx_offset as u32);
    packets(w, &b.tx);
    w.u32(b.tx_addr);
    w.u32(b.tx_len);
    w.u8(b.tx_type);
    w.u32(b.flash_addr);
    w.u32(b.flash_data);
    w.u32(b.flash_key);
    w.u32(b.flash_status);
    w.bool(b.flash_op.is_some());
    if let Some(o) = &b.flash_op {
        w.u64(o.end);
        w.u32(o.address);
        w.u32(o.value);
        w.bool(o.erase);
    }
    for v in &b.wear {
        w.u32(*v)
    }
    w.bool(b.reset_requested);
}
pub fn read_board(r: &mut Reader) -> Result<Board, String> {
    let mut b = Board::blank();
    b.cpu.r = read_registers(r)?;
    b.cpu.pc = r.u32()?;
    b.cpu.zero = r.bool()?;
    b.cpu.carry = r.bool()?;
    b.cpu.sleeping = r.bool()?;
    b.cpu.trapped = r.bool()?;
    b.cpu.irq_enabled = r.bool()?;
    b.cpu.cycles = r.u64()?;
    if r.bool()? {
        b.cpu.shadow = Some((read_registers(r)?, r.u32()?, r.bool()?, r.bool()?));
    }
    b.memory = r.bytes()?;
    if b.memory.len() != MEMORY {
        return Err("invalid memory image".into());
    }
    for _ in 0..r.count(64)? {
        let f = Fault {
            start: r.u32()?,
            end: r.u32()?,
            address_and: r.u32()?,
            data_and: r.u8()?,
            data_or: r.u8()?,
            write_inhibit: r.bool()?,
        };
        if f.start > f.end || f.end > MEMORY as u32 {
            return Err("invalid board configuration".into());
        }
        b.faults.push(f);
    }
    b.now = r.u64()?;
    b.boot_ms = r.u64()?;
    b.reset_count = r.u32()?;
    b.reset_cause = r.u32()?;
    b.watchdog = r.u64()?;
    b.timer_due = r.u64()?;
    b.pending_irq = r.bool()?;
    if b.boot_ms > b.now || b.now > 3_155_760_000_000 || b.watchdog <= b.now || b.timer_due <= b.now
    {
        return Err("invalid board clock".into());
    }
    b.rx = read_packets(r, 8)?;
    b.rx_offset = r.count(65536)?;
    b.tx = read_packets(r, 32)?;
    b.tx_addr = r.u32()?;
    b.tx_len = r.u32()?;
    b.tx_type = r.u8()?;
    b.flash_addr = r.u32()?;
    b.flash_data = r.u32()?;
    b.flash_key = r.u32()?;
    b.flash_status = r.u32()?;
    if r.bool()? {
        let o = FlashOp {
            end: r.u64()?,
            address: r.u32()?,
            value: r.u32()?,
            erase: r.bool()?,
        };
        if o.end <= b.now || o.address < 256 || o.address >= FLASH_END as u32 || o.address & 3 != 0
        {
            return Err("invalid pending device operation".into());
        }
        b.flash_op = Some(o);
    }
    for v in &mut b.wear {
        *v = r.u32()?;
    }
    b.reset_requested = r.bool()?;
    Ok(b)
}
fn write_air(w: &mut Writer, a: &Option<Air>) {
    w.bool(a.is_some());
    if let Some(a) = a {
        w.u64(a.end);
        w.u64(a.start);
        w.bytes(&a.wire);
        w.bool(a.failed);
        w.u64(a.window_end);
    }
}
fn read_air(r: &mut Reader) -> Result<Option<Air>, String> {
    Ok(if r.bool()? {
        Some(Air {
            end: r.u64()?,
            start: r.u64()?,
            wire: r.bytes()?,
            failed: r.bool()?,
            window_end: r.u64()?,
        })
    } else {
        None
    })
}
pub fn write_sim(w: &mut Writer, s: &Sim) {
    write_board(w, &s.board);
    w.u64(s.now);
    w.u64(s.pass.aos);
    w.u64(s.pass.los);
    w.u64(s.pass.peak.to_bits());
    packets(w, &s.queue);
    write_air(w, &s.uplink);
    write_air(w, &s.downlink);
    w.u32(s.next_seq as u32);
    w.u32(s.down_seq as u32);
    w.u64(s.beacon_due);
    w.u64(s.rng);
    w.u32(s.rate);
    w.bool(s.clean);
    w.u32(s.records.len() as u32);
    for a in &s.records {
        w.u64(a.time);
        w.u8(a.direction);
        w.string(&a.state);
        w.bytes(&a.wire);
    }
    w.u32(s.received.len() as u32);
    for (t, n, p) in &s.received {
        w.u64(*t);
        w.u32(*n as u32);
        w.bytes(p);
    }
}
pub fn read_sim(r: &mut Reader) -> Result<Sim, String> {
    let b = read_board(r)?;
    let mut s = Sim::new(b);
    s.now = r.u64()?;
    s.pass = Pass {
        aos: r.u64()?,
        los: r.u64()?,
        peak: f64::from_bits(r.u64()?),
    };
    s.queue = read_packets(r, 4096)?;
    s.uplink = read_air(r)?;
    s.downlink = read_air(r)?;
    s.next_seq = r.u32()? as u16;
    s.down_seq = r.u32()? as u16;
    s.beacon_due = r.u64()?;
    s.rng = r.u64()?;
    s.rate = r.u32()?;
    s.clean = r.bool()?;
    if s.now != s.board.now
        || s.pass.aos >= s.pass.los
        || s.pass.los <= s.now
        || s.beacon_due <= s.now
        || ![300, 1200].contains(&s.rate)
        || !s.pass.peak.is_finite()
    {
        return Err("invalid station clock".into());
    }
    for a in [&s.uplink, &s.downlink].into_iter().flatten() {
        if a.start > s.now || a.end <= s.now || a.wire.len() > 4096 {
            return Err("invalid transmission".into());
        }
    }
    for _ in 0..r.count(4096)? {
        s.records.push(Record {
            time: r.u64()?,
            direction: r.u8()?,
            state: r.string()?,
            wire: r.bytes()?,
        });
    }
    for _ in 0..r.count(2048)? {
        s.received.push((r.u64()?, r.u32()? as u16, r.bytes()?));
    }
    Ok(s)
}
