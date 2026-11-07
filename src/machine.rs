use std::collections::VecDeque;

pub const MEMORY: usize = 65536;
pub const FLASH_END: usize = 0x8000;
pub const RAM_START: usize = 0x8000;
pub const RAM_END: usize = 0xa000;
pub const NV_START: usize = 0xa000;
pub const NV_END: usize = 0xc000;
pub const CLOCK_HZ: u64 = 20_000;
pub const WATCHDOG_MS: u64 = 5000;

#[derive(Clone, Debug, PartialEq)]
pub struct Fault {
    pub start: u32,
    pub end: u32,
    pub address_and: u32,
    pub data_and: u8,
    pub data_or: u8,
    pub write_inhibit: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Cpu {
    pub r: [u32; 8],
    pub pc: u32,
    pub zero: bool,
    pub carry: bool,
    pub sleeping: bool,
    pub trapped: bool,
    pub irq_enabled: bool,
    pub shadow: Option<([u32; 8], u32, bool, bool)>,
    pub cycles: u64,
}
impl Default for Cpu {
    fn default() -> Self {
        let mut r = [0; 8];
        r[7] = 0x9ff0;
        Self {
            r,
            pc: 0,
            zero: false,
            carry: false,
            sleeping: false,
            trapped: false,
            irq_enabled: false,
            shadow: None,
            cycles: 0,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct FlashOp {
    pub end: u64,
    pub address: u32,
    pub value: u32,
    pub erase: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Board {
    pub cpu: Cpu,
    pub memory: Vec<u8>,
    pub faults: Vec<Fault>,
    pub now: u64,
    pub boot_ms: u64,
    pub reset_count: u32,
    pub reset_cause: u32,
    pub watchdog: u64,
    pub timer_due: u64,
    pub pending_irq: bool,
    pub rx: VecDeque<Vec<u8>>,
    pub rx_offset: usize,
    pub tx: VecDeque<Vec<u8>>,
    pub tx_addr: u32,
    pub tx_len: u32,
    pub tx_type: u8,
    pub flash_addr: u32,
    pub flash_data: u32,
    pub flash_key: u32,
    pub flash_status: u32,
    pub flash_op: Option<FlashOp>,
    pub wear: Vec<u32>,
    pub reset_requested: bool,
}
impl Board {
    pub fn blank() -> Self {
        let mut memory = vec![0; MEMORY];
        memory[..FLASH_END].fill(0xff);
        Self {
            cpu: Cpu::default(),
            memory,
            faults: vec![],
            now: 0,
            boot_ms: 0,
            reset_count: 0,
            reset_cause: 0,
            watchdog: WATCHDOG_MS,
            timer_due: 1000,
            pending_irq: false,
            rx: VecDeque::new(),
            rx_offset: 0,
            tx: VecDeque::new(),
            tx_addr: 0,
            tx_len: 0,
            tx_type: 0,
            flash_addr: 0,
            flash_data: 0,
            flash_key: 0,
            flash_status: 0,
            flash_op: None,
            wear: vec![0; 128],
            reset_requested: false,
        }
    }
    pub fn nominal(image: &[u8]) -> Self {
        let mut b = Self::blank();
        let n = image.len().min(FLASH_END);
        b.memory[..n].copy_from_slice(&image[..n]);
        b
    }
    pub fn reset(&mut self, cause: u32) {
        // Interrupted flash operations can leave partially changed storage.
        if let Some(op) = self.flash_op.take() {
            if op.erase {
                let p = op.address as usize & !255;
                for i in p..p + 128 {
                    if i >= 256 && i < FLASH_END {
                        self.memory[i] = 0xff;
                    }
                }
            } else {
                for (i, v) in op.value.to_le_bytes().iter().take(2).enumerate() {
                    if let Some(b) = self.memory.get_mut(op.address as usize + i) {
                        *b &= v;
                    }
                }
            }
        }
        self.memory[RAM_START..RAM_END].fill(0);
        self.cpu = Cpu::default();
        self.boot_ms = self.now;
        self.watchdog = self.now + WATCHDOG_MS;
        self.timer_due = self.now + 1000;
        self.reset_count = self.reset_count.wrapping_add(1);
        self.reset_cause = cause;
        self.pending_irq = false;
        self.rx.clear();
        self.rx_offset = 0;
        self.flash_key = 0;
        self.flash_status = 0;
        self.reset_requested = false;
    }
    fn physical(&self, address: u32) -> u32 {
        let mut a = address;
        for f in &self.faults {
            if address >= f.start && address < f.end {
                a &= f.address_and;
            }
        }
        a
    }
    pub fn read8(&mut self, address: u32) -> u8 {
        if address >= 0xf000 && address < 0xf100 {
            return self.io_read(address & !3).to_le_bytes()[(address & 3) as usize];
        }
        if (0xc000..0xf000).contains(&address) || address >= 0xf100 {
            return 0xff;
        }
        let a = self.physical(address);
        let mut v = self.memory.get(a as usize).copied().unwrap_or(0xff);
        for f in &self.faults {
            if address >= f.start && address < f.end {
                v = (v & f.data_and) | f.data_or;
            }
        }
        v
    }
    pub fn read32(&mut self, address: u32) -> u32 {
        if address >= 0xf000 && address < 0xf100 && address & 3 == 0 {
            return self.io_read(address);
        }
        u32::from_le_bytes(std::array::from_fn(|i| {
            self.read8(address.wrapping_add(i as u32))
        }))
    }
    pub fn write8(&mut self, address: u32, mut value: u8) {
        if !(RAM_START as u32..NV_END as u32).contains(&address) {
            return;
        }
        for f in &self.faults {
            if address >= f.start && address < f.end {
                if f.write_inhibit {
                    return;
                }
                value = (value & f.data_and) | f.data_or;
            }
        }
        let a = self.physical(address) as usize;
        if let Some(v) = self.memory.get_mut(a) {
            *v = value;
        }
    }
    pub fn write32(&mut self, address: u32, value: u32) {
        if (0xf000..0xf100).contains(&address) && address & 3 == 0 {
            self.io_write(address, value);
            return;
        }
        for (i, v) in value.to_le_bytes().iter().enumerate() {
            self.write8(address.wrapping_add(i as u32), *v);
        }
    }
    fn io_read(&mut self, a: u32) -> u32 {
        match a {
            0xf000 => self.rx.front().map_or(0, |p| p.len() as u32),
            0xf004 => {
                let v = self
                    .rx
                    .front()
                    .and_then(|p| p.get(self.rx_offset))
                    .copied()
                    .unwrap_or(0);
                self.rx_offset += 1;
                v as u32
            }
            0xf020 => (self.now - self.boot_ms) as u32,
            0xf024 => self.reset_count,
            0xf028 => self.reset_cause,
            0xf030 => self.flash_status | if self.flash_op.is_some() { 1 } else { 0 },
            0xf050 => 7600 + ((self.now / 1000) % 160) as u32,
            0xf054 => 2100 + ((self.now / 3000) % 70) as u32,
            0xf058 => 100 + ((self.now / 1700) % 500) as u32,
            _ => 0,
        }
    }
    fn io_write(&mut self, a: u32, v: u32) {
        match a {
            0xf008 => {
                self.rx.pop_front();
                self.rx_offset = 0;
            }
            0xf010 => self.tx_addr = v,
            0xf014 => self.tx_len = v.min(256),
            0xf01c => self.tx_type = v as u8,
            0xf018 => {
                if self.tx.len() < 32 {
                    let mut p = vec![self.tx_type];
                    for i in 0..self.tx_len {
                        p.push(self.read8(self.tx_addr.wrapping_add(i)));
                    }
                    self.tx.push_back(p);
                }
            }
            0xf02c => {
                if v == 0x51ac {
                    self.watchdog = self.now + WATCHDOG_MS;
                }
            }
            0xf034 => self.flash_addr = v,
            0xf038 => self.flash_data = v,
            0xf03c => self.flash_key = v,
            0xf030 => {
                self.flash_status = 0;
                if self.flash_op.is_some()
                    || self.flash_key != 0xc0de
                    || self.flash_addr < 256
                    || self.flash_addr >= FLASH_END as u32
                    || self.flash_addr & 3 != 0
                    || ![1, 2].contains(&v)
                {
                    self.flash_status = 2;
                } else {
                    self.flash_op = Some(FlashOp {
                        end: self.now + if v == 1 { 30 } else { 2 },
                        address: self.flash_addr,
                        value: self.flash_data,
                        erase: v == 1,
                    });
                }
                self.flash_key = 0;
            }
            0xf040 => {
                if v == 1 {
                    self.reset_requested = true;
                }
            }
            _ => {}
        }
    }
    pub fn receive(&mut self, p: Vec<u8>) -> bool {
        if self.rx.len() >= 8 {
            return false;
        }
        self.rx.push_back(p);
        self.cpu.sleeping = false;
        true
    }
    pub fn advance_devices(&mut self, now: u64) {
        self.now = now;
        if self.flash_op.as_ref().is_some_and(|op| op.end <= now) {
            let op = self.flash_op.take().unwrap();
            if op.erase {
                let p = op.address as usize & !255;
                self.wear[p / 256] = self.wear[p / 256].saturating_add(1);
                if self.wear[p / 256] <= 10000 {
                    self.memory[p..p + 256].fill(0xff);
                } else {
                    self.flash_status = 4;
                }
            } else {
                for (i, v) in op.value.to_le_bytes().iter().enumerate() {
                    let b = &mut self.memory[op.address as usize + i];
                    if (*b & v) != *v {
                        self.flash_status = 2;
                    }
                    *b &= v;
                }
            }
        }
        if now >= self.watchdog || self.reset_requested {
            self.reset(if self.reset_requested { 2 } else { 1 });
        }
        if now >= self.timer_due {
            self.pending_irq = true;
            self.timer_due += 1000;
            self.cpu.sleeping = false;
        }
    }
    pub fn step(&mut self) {
        if self.cpu.trapped {
            return;
        }
        if self.pending_irq && self.cpu.irq_enabled && self.cpu.shadow.is_none() {
            self.cpu.shadow = Some((self.cpu.r, self.cpu.pc, self.cpu.zero, self.cpu.carry));
            self.cpu.r = [0; 8];
            self.cpu.r[7] = 0x9ef0;
            self.cpu.pc = 8;
            self.cpu.sleeping = false;
            self.pending_irq = false;
        }
        if self.cpu.sleeping {
            return;
        }
        let pc = self.cpu.pc;
        if pc & 7 != 0 || pc > 0xbff8 {
            self.cpu.trapped = true;
            return;
        }
        let op = self.read8(pc);
        let a = self.read8(pc + 1) as usize;
        let b = self.read8(pc + 2) as usize;
        let c = self.read8(pc + 3) as usize;
        let imm = self.read32(pc + 4);
        if a >= 8 || b >= 8 || c >= 8 {
            self.cpu.trapped = true;
            return;
        }
        self.cpu.pc += 8;
        self.cpu.cycles += 1;
        let rb = self.cpu.r[b];
        let rc = self.cpu.r[c];
        match op {
            0 => {}
            1 => self.cpu.r[a] = imm,
            2 => self.cpu.r[a] = rb,
            3 => {
                let (v, carry) = rb.overflowing_add(rc);
                self.cpu.r[a] = v;
                self.cpu.carry = carry;
            }
            4 => self.cpu.r[a] = rb.wrapping_add(imm),
            5 => {
                self.cpu.r[a] = rb.wrapping_sub(rc);
                self.cpu.carry = rb >= rc;
            }
            6 => self.cpu.r[a] = rb & rc,
            7 => self.cpu.r[a] = rb | rc,
            8 => self.cpu.r[a] = rb ^ rc,
            9 => self.cpu.r[a] = rb.wrapping_shl(imm & 31),
            10 => self.cpu.r[a] = rb.wrapping_shr(imm & 31),
            11 => self.cpu.r[a] = self.read8(rb.wrapping_add(imm)) as u32,
            12 => self.cpu.r[a] = self.read32(rb.wrapping_add(imm)),
            13 => self.write8(rb.wrapping_add(imm), self.cpu.r[a] as u8),
            14 => self.write32(rb.wrapping_add(imm), self.cpu.r[a]),
            15 => {
                self.cpu.zero = self.cpu.r[a] == rb;
                self.cpu.carry = self.cpu.r[a] >= rb;
            }
            16 => {
                self.cpu.zero = self.cpu.r[a] == imm;
                self.cpu.carry = self.cpu.r[a] >= imm;
            }
            17 => self.cpu.pc = imm,
            18 => {
                if self.cpu.zero {
                    self.cpu.pc = imm;
                }
            }
            19 => {
                if !self.cpu.zero {
                    self.cpu.pc = imm;
                }
            }
            20 => {
                if !self.cpu.carry {
                    self.cpu.pc = imm;
                }
            }
            21 => {
                if self.cpu.carry {
                    self.cpu.pc = imm;
                }
            }
            22 | 24 => {
                let dest = if op == 22 { imm } else { self.cpu.r[a] };
                self.push(self.cpu.pc);
                self.cpu.pc = dest;
            }
            23 => self.cpu.pc = self.pop(),
            25 => self.push(self.cpu.r[a]),
            26 => self.cpu.r[a] = self.pop(),
            27 => self.cpu.sleeping = true,
            28 => self.cpu.irq_enabled = true,
            29 => self.cpu.irq_enabled = false,
            30 => {
                if let Some((r, p, z, c)) = self.cpu.shadow.take() {
                    self.cpu.r = r;
                    self.cpu.pc = p;
                    self.cpu.zero = z;
                    self.cpu.carry = c;
                } else {
                    self.cpu.trapped = true;
                }
            }
            31 => self.cpu.trapped = true,
            32 => self.cpu.pc = self.cpu.r[a],
            33 => self.cpu.r[a] = rb.wrapping_mul(rc),
            _ => self.cpu.trapped = true,
        }
    }
    fn push(&mut self, v: u32) {
        self.cpu.r[7] = self.cpu.r[7].wrapping_sub(4);
        self.write32(self.cpu.r[7], v);
    }
    fn pop(&mut self) -> u32 {
        let v = self.read32(self.cpu.r[7]);
        self.cpu.r[7] = self.cpu.r[7].wrapping_add(4);
        v
    }
    pub fn run_cycles(&mut self, cycles: u64, breakpoint: Option<u32>) {
        for _ in 0..cycles {
            if breakpoint == Some(self.cpu.pc) {
                break;
            }
            if self.cpu.trapped
                || (self.cpu.sleeping
                    && !(self.pending_irq && self.cpu.irq_enabled && self.cpu.shadow.is_none()))
            {
                break;
            }
            self.step();
            if self.reset_requested {
                self.reset(2);
            }
        }
    }
    pub fn advance(&mut self, duration: u64, breakpoint: Option<u32>) {
        let end = self.now + duration;
        while self.now < end {
            let next = if self.cpu.sleeping || self.cpu.trapped {
                end.min(self.timer_due)
                    .min(self.watchdog)
                    .min(self.flash_op.as_ref().map_or(end, |o| o.end))
            } else {
                (self.now + 1).min(end)
            };
            let delta = next.saturating_sub(self.now).max(1);
            self.advance_devices((self.now + delta).min(end));
            self.run_cycles(CLOCK_HZ / 1000, breakpoint);
            if breakpoint == Some(self.cpu.pc) {
                break;
            }
        }
    }
}
