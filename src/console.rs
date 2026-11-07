use crate::{
    asm,
    codec::{self, Reader, Writer},
    link::{self, Sim},
    machine::{Board, Fault},
    orbit,
};
use std::collections::BTreeMap;
include!(concat!(env!("OUT_DIR"), "/archive.rs"));
pub const BRIEFING:&str="ORIEL-1 / LINDEN UNIVERSITY / RECOVERY CAMPAIGN\n\nLaunched in 2014. Last accepted engineering telemetry: 2016.\nRecent passes still show a carrier and intermittent command responses.\nThe original flight team has dispersed. The archive is what survived.\n\nYou have a simulated ground station, the engineering archive, and a local\nbench reconstructed from the last archived release. Nothing leaves this\ncomputer. Time advances only when you request it.\n\nEstablish what is functioning. Keep an engineering record. If possible,\nrestore trustworthy telemetry and sustained spacecraft operation.\n\nWe don't know what's wrong with it. Here's everything we have.\nSee if you can bring it back.\n\nStart with: help · files · cat archive/operations.txt\n";
pub const HELP:&str="GROUND / COMMAND REFERENCE\n\nbriefing                         Mission handover\ntrack | passes [count]           Link geometry and upcoming contacts\nnext                             Advance to next acquisition of signal + 5 s\nwait SECONDS                     Advance all clocks (0..86400 per command)\nrate 300|1200                    Set simulated uplink bit rate\nsend ping|read ADDR LEN|write ADDR HEX|service ID|reset\nsend update begin|chunk HEX|commit\nsend hex HEX                     Queue arbitrary command payload\nraw HEX                          Queue a complete frame, including CRC\nframe HEX                        Construct a frame locally\nupload FILE ADDR [OFFSET COUNT]  Queue 48-byte RAM write commands\nqueue | cancel                   Inspect / discard unsent uplink frames\nrx [count] | decode [SEQ|last]    Received packets / local telemetry decoder\nlog [count]                      Raw station traffic with timestamps\nextract SEQ FILE                 Save response data (without type byte)\nfiles | cat FILE | hex FILE [OFFSET COUNT]\nput FILE HEX | write FILE TEXT   Create workspace binary / text (\\n supported)\nasm SOURCE BASE OUTPUT          Assemble a workspace source; emits OUTPUT.sym\ndisasm FILE BASE [OFFSET COUNT]  Disassemble locally (count = instructions)\ncrc FILE                        CRC-16/CCITT-FALSE of a local file\nlab help                        Local bench debugger (never the spacecraft)\nsnapshot NAME | restore NAME | snapshots\nnew                              Restart investigation (normal mode only)\nhelp                             This reference\n\nNumbers accept decimal or 0x hex; HEX is hex bytes without a 0x prefix.\nQueued commands do nothing until time advances. Responses are asynchronous.\nCLI only: import PATH [NAME], export NAME PATH, quit.\n";
#[derive(Clone)]
pub struct Console {
    sim: Sim,
    lab: Board,
    files: BTreeMap<String, Vec<u8>>,
    snapshots: BTreeMap<String, Vec<u8>>,
    hardcore: bool,
    breakpoint: Option<u32>,
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn unhex(s: &str) -> Result<Vec<u8>, String> {
    let s: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .collect();
    if s.len() % 2 != 0 || !s.is_ascii() {
        return Err("hex requires pairs of ASCII digits".into());
    }
    if s.len() > 2 * 1024 * 1024 {
        return Err("payload too large".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "invalid hex".into()))
        .collect()
}
fn num(s: &str) -> Result<u32, String> {
    asm::number(s)
}
fn at<'a>(w: &[&'a str], i: usize) -> Result<&'a str, String> {
    w.get(i)
        .copied()
        .ok_or_else(|| "missing argument; type help".into())
}
fn bounded(s: &str, max: u32) -> Result<u32, String> {
    let n = num(s)?;
    if n > max {
        Err(format!("value exceeds {max}"))
    } else {
        Ok(n)
    }
}
fn stamp(t: u64) -> String {
    format!(
        "T+{:03}:{:02}:{:02}.{:03}",
        t / 3600000,
        t / 60000 % 60,
        t / 1000 % 60,
        t % 1000
    )
}
fn dump(data: &[u8], base: usize) -> String {
    let mut s = String::new();
    for (i, c) in data.chunks(16).enumerate() {
        let h = c.iter().map(|b| format!("{b:02x} ")).collect::<String>();
        let a = c
            .iter()
            .map(|b| {
                if (32..127).contains(b) {
                    *b as char
                } else {
                    '.'
                }
            })
            .collect::<String>();
        s.push_str(&format!("{:04x}  {h:48} {a}\n", base + i * 16));
    }
    s
}
impl Console {
    pub fn new(hardcore: bool) -> Result<Self, String> {
        let bytes = codec::open(include_bytes!("../assets/mission.orb"))?;
        let mut r = Reader::new(&bytes);
        let b = codec::read_board(&mut r)?;
        r.done()?;
        let image = ARCHIVE
            .iter()
            .find(|(n, _)| *n == "archive/flight.bin")
            .map(|(_, b)| *b)
            .ok_or("flight archive missing")?;
        Ok(Self {
            sim: Sim::new(b),
            lab: Board::nominal(image),
            files: BTreeMap::new(),
            snapshots: BTreeMap::new(),
            hardcore,
            breakpoint: None,
        })
    }
    pub fn hardcore(&self) -> bool {
        self.hardcore
    }
    pub fn enable_hardcore(&mut self) {
        self.hardcore = true;
        self.snapshots.clear();
    }
    pub fn file(&self, name: &str) -> Result<&[u8], String> {
        self.files
            .get(name)
            .map(Vec::as_slice)
            .or_else(|| ARCHIVE.iter().find(|(n, _)| *n == name).map(|(_, b)| *b))
            .ok_or_else(|| format!("file not found: {name}"))
    }
    pub fn put(&mut self, name: &str, bytes: Vec<u8>) -> Result<(), String> {
        if name.is_empty()
            || name.starts_with("archive/")
            || name.len() > 128
            || name.contains("..")
            || name.chars().any(char::is_whitespace)
        {
            return Err("choose a workspace name; archive is read-only".into());
        }
        if self.files.len() >= 128 && !self.files.contains_key(name) {
            return Err("workspace file limit reached".into());
        }
        let old = self.files.get(name).map_or(0, Vec::len);
        if bytes.len() > 1024 * 1024
            || self.files.values().map(Vec::len).sum::<usize>() - old + bytes.len()
                > 4 * 1024 * 1024
        {
            return Err("workspace byte limit reached".into());
        }
        self.files.insert(name.into(), bytes);
        Ok(())
    }
    pub fn exec(&mut self, line: &str) -> String {
        match self.command(line) {
            Ok(s) => s,
            Err(e) => format!("error: {e}\n"),
        }
    }
    fn command(&mut self, line: &str) -> Result<String, String> {
        let w: Vec<_> = line.split_whitespace().collect();
        if w.is_empty() {
            return Ok(String::new());
        }
        match w[0] {
            "help" => Ok(HELP.into()),
            "briefing" => Ok(BRIEFING.into()),
            "track" => Ok(self.track()),
            "passes" => {
                let n = bounded(w.get(1).unwrap_or(&"6"), 24)?;
                Ok(orbit::predictions(self.sim.now, n as usize)
                    .iter()
                    .map(|p| {
                        format!(
                            "{} → {}  {:5.1}°  {:.1} s\n",
                            stamp(p.aos),
                            stamp(p.los),
                            p.peak,
                            (p.los - p.aos) as f64 / 1000.0
                        )
                    })
                    .collect())
            }
            "next" => {
                let p = if self.sim.contact() {
                    orbit::next_pass(self.sim.pass.los + 1)
                } else {
                    self.sim.pass
                };
                self.sim.advance(p.aos + 5000 - self.sim.now);
                Ok(self.track())
            }
            "wait" => {
                let sec: f64 = at(&w, 1)?.parse().map_err(|_| "seconds must be a number")?;
                if !sec.is_finite() || !(0.0..=86400.0).contains(&sec) {
                    return Err("wait range is 0..86400 seconds".into());
                }
                self.sim.advance((sec * 1000.0).round() as u64);
                Ok(self.track())
            }
            "rate" => {
                let n = num(at(&w, 1)?)?;
                if ![300, 1200].contains(&n) {
                    return Err("supported uplink rates: 300, 1200".into());
                }
                self.sim.rate = n;
                Ok(format!(
                    "Uplink {n} bit/s. In-flight frames retain their scheduled duration.\n"
                ))
            }
            "send" => {
                let p = Self::payload(&w[1..])?;
                self.enqueue(p)
            }
            "raw" => {
                let bytes = unhex(&w[1..].join(""))?;
                if bytes.is_empty() || bytes.len() > 265 {
                    return Err("raw frame must be 1..265 bytes".into());
                }
                self.check_queue(1)?;
                self.sim.queue.push_back(bytes);
                Ok("Raw frame queued.\n".into())
            }
            "frame" => {
                let p = unhex(&w[1..].join(""))?;
                if p.len() > 257 {
                    return Err("payload limit is 257 bytes".into());
                }
                Ok(format!("{}\n", hex(&link::frame(self.sim.next_seq, &p))))
            }
            "queue" => Ok(format!(
                "{} pending frame(s); uplink {}; downlink {}\n",
                self.sim.queue.len(),
                if self.sim.uplink.is_some() {
                    "busy"
                } else {
                    "idle"
                },
                if self.sim.downlink.is_some() {
                    "busy"
                } else {
                    "idle"
                }
            )),
            "cancel" => {
                let n = self.sim.queue.len();
                self.sim.queue.clear();
                Ok(format!("Cancelled {n} unsent frames.\n"))
            }
            "upload" => {
                let file = at(&w, 1)?;
                let addr = bounded(at(&w, 2)?, 65535)?;
                let off = num(w.get(3).unwrap_or(&"0"))? as usize;
                let f = self.file(file)?;
                let count = if let Some(s) = w.get(4) {
                    num(s)? as usize
                } else {
                    f.len().saturating_sub(off)
                };
                let b = f
                    .get(off..off.checked_add(count).ok_or("range overflow")?)
                    .ok_or("file range out of bounds")?
                    .to_vec();
                if addr as usize + count > 65536 {
                    return Err("upload crosses address space".into());
                }
                self.check_queue(count.div_ceil(48))?;
                for (i, c) in b.chunks(48).enumerate() {
                    let mut p = vec![3];
                    p.extend(((addr as usize + i * 48) as u16).to_le_bytes());
                    p.extend(c);
                    self.sim.queue(p);
                }
                Ok(format!(
                    "Queued {} bytes in {} frames. Inspect acknowledgements before retrying.\n",
                    b.len(),
                    b.len().div_ceil(48)
                ))
            }
            "rx" => {
                let n = bounded(w.get(1).unwrap_or(&"12"), 256)? as usize;
                Ok(self
                    .sim
                    .received
                    .iter()
                    .rev()
                    .take(n)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|(t, s, p)| format!("{} RX #{s:05} {}\n", stamp(*t), hex(p)))
                    .collect())
            }
            "decode" => {
                let p = if w.get(1).is_none_or(|s| *s == "last") {
                    self.sim.received.last()
                } else {
                    let n = bounded(at(&w, 1)?, 65535)? as u16;
                    self.sim.received.iter().rev().find(|(_, s, _)| *s == n)
                }
                .ok_or("no matching received packet")?;
                Ok(format!("{} #{} {}\n", stamp(p.0), p.1, decode(&p.2)))
            }
            "log" => {
                let n = bounded(w.get(1).unwrap_or(&"30"), 4096)? as usize;
                Ok(self
                    .sim
                    .records
                    .iter()
                    .rev()
                    .take(n)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|r| {
                        format!(
                            "{} {} {:18} {}\n",
                            stamp(r.time),
                            if r.direction == 0 { "UP" } else { "DN" },
                            r.state,
                            hex(&r.wire)
                        )
                    })
                    .collect())
            }
            "extract" => {
                let n = bounded(at(&w, 1)?, 65535)? as u16;
                let p = self
                    .sim
                    .received
                    .iter()
                    .rev()
                    .find(|(_, s, _)| *s == n)
                    .ok_or("no matching packet")?
                    .2
                    .clone();
                self.put(at(&w, 2)?, p.get(1..).unwrap_or(&[]).to_vec())?;
                Ok("Response data saved.\n".into())
            }
            "files" => {
                let mut s = String::new();
                for (n, b) in ARCHIVE {
                    s.push_str(&format!("{:7}  {n}\n", b.len()));
                }
                for (n, b) in &self.files {
                    s.push_str(&format!("{:7}  {n}\n", b.len()));
                }
                Ok(s)
            }
            "cat" => {
                let b = self.file(at(&w, 1)?)?;
                let s = std::str::from_utf8(b).map_err(|_| "binary file; use hex or disasm")?;
                if s.len() > 128 * 1024 {
                    return Err("text is too large for terminal".into());
                }
                Ok(format!("{s}\n"))
            }
            "hex" => {
                let f = self.file(at(&w, 1)?)?;
                let off = num(w.get(2).unwrap_or(&"0"))? as usize;
                let n = bounded(w.get(3).unwrap_or(&"256"), 4096)? as usize;
                let b = f
                    .get(off..off.saturating_add(n).min(f.len()))
                    .ok_or("offset outside file")?;
                Ok(dump(b, off))
            }
            "put" => {
                self.put(at(&w, 1)?, unhex(&w[2..].join(""))?)?;
                Ok("Workspace file saved.\n".into())
            }
            "write" => {
                let name = at(&w, 1)?;
                let text = line
                    .trim_start()
                    .strip_prefix("write")
                    .unwrap()
                    .trim_start()
                    .strip_prefix(name)
                    .unwrap()
                    .trim_start()
                    .replace("\\n", "\n");
                self.put(name, text.into_bytes())?;
                Ok("Workspace file saved.\n".into())
            }
            "asm" => {
                let source = std::str::from_utf8(self.file(at(&w, 1)?)?)
                    .map_err(|_| "source is not UTF-8")?;
                let a = asm::assemble(source, num(at(&w, 2)?)?)?;
                let name = at(&w, 3)?;
                let n = a.bytes.len();
                self.put(name, a.bytes)?;
                let symbols = a
                    .symbols
                    .iter()
                    .map(|(n, v)| format!("{v:08x} {n}\n"))
                    .collect::<String>();
                self.put(&format!("{name}.sym"), symbols.into_bytes())?;
                Ok(format!("Assembled {n} bytes at 0x{:04x}.\n", a.base))
            }
            "disasm" => {
                let f = self.file(at(&w, 1)?)?;
                let base = num(at(&w, 2)?)?;
                let off = num(w.get(3).unwrap_or(&"0"))? as usize;
                let n = bounded(w.get(4).unwrap_or(&"32"), 1024)? as usize;
                Ok(asm::disassemble(
                    f.get(off..).ok_or("offset outside file")?,
                    base.checked_add(off as u32).ok_or("address overflow")?,
                    n,
                ))
            }
            "crc" => Ok(format!(
                "CRC-16/CCITT-FALSE {:04x}\n",
                link::crc16(self.file(at(&w, 1)?)?)
            )),
            "lab" => self.lab_command(&w[1..]),
            "snapshot" => {
                if self.hardcore {
                    return Err("snapshots are disabled in hardcore mode".into());
                }
                let name = at(&w, 1)?;
                if name.len() > 64
                    || self.snapshots.len() >= 8 && !self.snapshots.contains_key(name)
                {
                    return Err("snapshot limit: eight slots, names up to 64 bytes".into());
                }
                let b = self.encode(false);
                self.snapshots.insert(name.into(), b);
                Ok(format!("Snapshot '{name}' saved.\n"))
            }
            "restore" => {
                if self.hardcore {
                    return Err("restore is disabled in hardcore mode".into());
                }
                let b = self.snapshots.get(at(&w, 1)?).ok_or("snapshot not found")?;
                let mut c = Self::load(b)?;
                std::mem::swap(&mut c.snapshots, &mut self.snapshots);
                *self = c;
                Ok(format!("Snapshot restored. {}", self.track()))
            }
            "snapshots" => Ok(format!(
                "{}\n",
                self.snapshots
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            )),
            "new" => {
                if self.hardcore {
                    return Err("restart is disabled in hardcore mode".into());
                }
                *self = Self::new(false)?;
                Ok(BRIEFING.into())
            }
            _ => Err("unknown command; type help".into()),
        }
    }
    fn check_queue(&self, n: usize) -> Result<(), String> {
        if self.sim.queue.len() + n > 4096 {
            Err("ground queue is full".into())
        } else {
            Ok(())
        }
    }
    fn enqueue(&mut self, p: Vec<u8>) -> Result<String, String> {
        if p.is_empty() || p.len() > 257 {
            return Err("command payload must be 1..257 bytes".into());
        }
        self.check_queue(1)?;
        let n = self.sim.queue(p);
        Ok(format!("Queued uplink #{n:05}.\n"))
    }
    fn payload(w: &[&str]) -> Result<Vec<u8>, String> {
        match at(w, 0)? {
            "ping" => Ok(vec![1]),
            "read" => {
                let mut p = vec![2];
                p.extend((bounded(at(w, 1)?, 65535)? as u16).to_le_bytes());
                p.extend((bounded(at(w, 2)?, 128)? as u16).to_le_bytes());
                Ok(p)
            }
            "write" => {
                let mut p = vec![3];
                p.extend((bounded(at(w, 1)?, 65535)? as u16).to_le_bytes());
                let b = unhex(&w[2..].join(""))?;
                if b.is_empty() || b.len() > 128 {
                    return Err("write requires 1..128 bytes".into());
                }
                p.extend(b);
                Ok(p)
            }
            "service" => {
                let mut p = vec![5];
                p.extend((bounded(at(w, 1)?, 65535)? as u16).to_le_bytes());
                Ok(p)
            }
            "reset" => Ok(vec![6]),
            "update" => match at(w, 1)? {
                "begin" => Ok(vec![7]),
                "chunk" => {
                    let mut p = vec![8];
                    p.extend(unhex(&w[2..].join(""))?);
                    Ok(p)
                }
                "commit" => Ok(vec![9]),
                _ => Err("update begin|chunk HEX|commit".into()),
            },
            "hex" => unhex(&w[1..].join("")),
            _ => Err("unknown command encoder; use send hex for raw payloads".into()),
        }
    }
    fn track(&self) -> String {
        format!(
            "{}  {}  elevation {:.1}°\n{}  uplink {} bit/s · {} queued · {} received\n",
            stamp(self.sim.now),
            if self.sim.contact() {
                "IN CONTACT"
            } else {
                "NO CONTACT"
            },
            orbit::elevation(self.sim.now),
            if self.sim.contact() {
                format!(
                    "LOS in {:.3} s",
                    (self.sim.pass.los - self.sim.now) as f64 / 1000.0
                )
            } else {
                format!(
                    "AOS in {:.3} s",
                    (self.sim.pass.aos - self.sim.now) as f64 / 1000.0
                )
            },
            self.sim.rate,
            self.sim.queue.len(),
            self.sim.received.len()
        )
    }
    fn lab_command(&mut self, w: &[&str]) -> Result<String, String> {
        match w.first().copied().unwrap_or("help"){
        "help"=>Ok("LOCAL BENCH / independent nominal engineering model\nlab reset | regs | step [COUNT] | run MILLISECONDS\nlab mem ADDR LEN | load FILE ADDR | pc ADDR | break ADDR|off\nlab inject HEX            Deliver a command payload on the bench\nlab rx                    Inspect bench output packets\nlab fault START END ADDRESS_AND DATA_AND DATA_OR [inhibit]\nlab clear-faults          Remove your bench fault hypotheses\nAddresses and masks accept 0x hex. Lab time is independent of flight time.\n".into()),
        "reset"=>{let image=self.file("archive/flight.bin")?.to_vec();self.lab=Board::nominal(&image);self.breakpoint=None;Ok("Bench restored from archived release.\n".into())},
        "regs"=>Ok(format!("BENCH t={} ms pc=0x{:04x} Z={} C={} asleep={} trap={} cycles={} resets={}\n{}\n",self.lab.now,self.lab.cpu.pc,self.lab.cpu.zero,self.lab.cpu.carry,self.lab.cpu.sleeping,self.lab.cpu.trapped,self.lab.cpu.cycles,self.lab.reset_count,self.lab.cpu.r.iter().enumerate().map(|(i,r)|format!("r{i}={r:08x}")).collect::<Vec<_>>().join(" "))),
        "step"=>{let n=bounded(w.get(1).unwrap_or(&"1"),100000)?;for _ in 0..n{self.lab.step();}self.lab_command(&["regs"])},
        "run"=>{self.lab.advance(bounded(at(w,1)?,86400000)?as u64,self.breakpoint);self.lab_command(&["regs"])},
        "mem"=>{let a=bounded(at(w,1)?,65535)?;let n=bounded(at(w,2)?,4096)?;let b=(0..n).map(|i|self.lab.read8(a+i)).collect::<Vec<_>>();Ok(dump(&b,a as usize))},
        "load"=>{let b=self.file(at(w,1)?)?.to_vec();let a=bounded(at(w,2)?,65535)?as usize;let target=self.lab.memory.get_mut(a..a+b.len()).ok_or("image outside bench memory")?;target.copy_from_slice(&b);Ok("Loaded into bench memory.\n".into())},
        "pc"=>{let a=bounded(at(w,1)?,65535)?;if a&7!=0{return Err("PC must be aligned to eight bytes".into());}self.lab.cpu.pc=a;self.lab.cpu.trapped=false;self.lab.cpu.sleeping=false;Ok("Bench PC set.\n".into())},
        "break"=>{self.breakpoint=if at(w,1)?=="off"{None}else{Some(bounded(at(w,1)?,65535)?)};Ok("Bench breakpoint updated.\n".into())},
        "inject"=>{let b=unhex(&w[1..].join(""))?;if b.len()>257{return Err("payload too long".into());}if !self.lab.receive(b){return Err("bench RX FIFO full".into());}Ok("Bench input queued.\n".into())},
        "rx"=>{let s=self.lab.tx.iter().map(|p|format!("{}  {}\n",hex(p),decode(p))).collect();self.lab.tx.clear();Ok(s)},
        "fault"=>{let f=Fault{start:bounded(at(w,1)?,65536)?,end:bounded(at(w,2)?,65536)?,address_and:num(at(w,3)?)?,data_and:bounded(at(w,4)?,255)?as u8,data_or:bounded(at(w,5)?,255)?as u8,write_inhibit:w.get(6)==Some(&"inhibit")};if f.start>=f.end||self.lab.faults.len()>=64{return Err("invalid bench fault range or limit".into());}self.lab.faults.push(f);Ok("Bench hypothesis installed.\n".into())},
        "clear-faults"=>{self.lab.faults.clear();Ok("Bench hypotheses cleared.\n".into())},_=>Err("unknown bench command; lab help".into())}
    }
    pub fn save(&self) -> Vec<u8> {
        self.encode(true)
    }
    fn encode(&self, slots: bool) -> Vec<u8> {
        let mut w = Writer(vec![]);
        w.u32(1);
        w.bool(self.hardcore);
        codec::write_sim(&mut w, &self.sim);
        codec::write_board(&mut w, &self.lab);
        w.bool(self.breakpoint.is_some());
        if let Some(p) = self.breakpoint {
            w.u32(p);
        }
        w.u32(self.files.len() as u32);
        for (n, b) in &self.files {
            w.string(n);
            w.bytes(b);
        }
        w.u32(if slots {
            self.snapshots.len() as u32
        } else {
            0
        });
        if slots {
            for (n, b) in &self.snapshots {
                w.string(n);
                w.bytes(b);
            }
        }
        codec::seal(&w.0)
    }
    pub fn load(bytes: &[u8]) -> Result<Self, String> {
        let b = codec::open(bytes)?;
        let mut r = Reader::new(&b);
        if r.u32()? != 1 {
            return Err("unsupported session version".into());
        }
        let hardcore = r.bool()?;
        let sim = codec::read_sim(&mut r)?;
        let lab = codec::read_board(&mut r)?;
        let breakpoint = if r.bool()? { Some(r.u32()?) } else { None };
        let mut c = Self {
            sim,
            lab,
            hardcore,
            breakpoint,
            files: BTreeMap::new(),
            snapshots: BTreeMap::new(),
        };
        for _ in 0..r.count(128)? {
            let n = r.string()?;
            let b = r.bytes()?;
            c.put(&n, b)?;
        }
        for _ in 0..r.count(8)? {
            let n = r.string()?;
            let b = r.bytes()?;
            c.snapshots.insert(n, b);
        }
        r.done()?;
        Ok(c)
    }
    pub fn status_json(&self) -> String {
        format!("{{\"time\":{},\"contact\":{},\"elevation\":{:.2},\"aos\":{},\"los\":{},\"queued\":{},\"received\":{},\"hardcore\":{}}}",self.sim.now,self.sim.contact(),orbit::elevation(self.sim.now),self.sim.pass.aos,self.sim.pass.los,self.sim.queue.len(),self.sim.received.len(),self.hardcore)
    }
}
pub fn decode(p: &[u8]) -> String {
    let word = |off: usize| -> u32 {
        p.get(off..off + 4)
            .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    match p.first().copied() {
        Some(0x80) if p.len() == 9 => {
            format!("LINK BEACON uptime={} s reset_count={}", word(1), word(5))
        }
        Some(0x81) => format!("ECHO {}", hex(&p[1..])),
        Some(0x82) => format!("MEMORY {}", hex(&p[1..])),
        Some(0x83) => format!("ACK {}", hex(&p[1..])),
        Some(0xe0) => format!("FLIGHT STATUS {}", hex(&p[1..])),
        Some(0x90) if p.len() == 41 => {
            let sum = (0..9).fold(0u32, |s, i| s.wrapping_add(word(1 + i * 4)));
            if word(1) != 0x314b484f || sum != word(37) {
                format!(
                    "ENGINEERING invalid record: signature={:08x} integrity={} raw={}",
                    word(1),
                    sum == word(37),
                    hex(&p[1..])
                )
            } else {
                format!("ENGINEERING valid uptime={} ms bus={} mV temperature={:.2} C resets={} samples={} mode={} fw={:04x} sensor={}",word(5),word(9),word(13)as f64/100.0,word(17),word(21),word(25),word(29),word(33))
            }
        }
        Some(t) => format!("TYPE {t:02x} {} byte(s): {}", p.len() - 1, hex(&p[1..])),
        None => "empty packet".into(),
    }
}
