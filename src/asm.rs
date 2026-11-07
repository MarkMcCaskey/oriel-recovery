use std::collections::BTreeMap;
#[derive(Debug)]
pub struct Assembly {
    pub base: u32,
    pub bytes: Vec<u8>,
    pub symbols: BTreeMap<String, u32>,
}
pub fn number(s: &str) -> Result<u32, String> {
    if let Some(v) = s.strip_prefix('-') {
        return Ok(0u32.wrapping_sub(number(v)?));
    }
    let s = s.replace('_', "");
    if let Some(v) = s.strip_prefix("0x") {
        u32::from_str_radix(v, 16)
    } else {
        s.parse()
    }
    .map_err(|_| format!("invalid number: {s}"))
}
fn value(s: &str, symbols: &BTreeMap<String, u32>) -> Result<u32, String> {
    symbols.get(s).copied().map(Ok).unwrap_or_else(|| number(s))
}
fn reg(s: &str) -> Result<u8, String> {
    let n = s
        .strip_prefix('r')
        .ok_or_else(|| format!("expected register, got {s}"))?;
    let n = number(n)?;
    if n < 8 {
        Ok(n as u8)
    } else {
        Err("register must be r0..r7".into())
    }
}
fn words(line: &str) -> Vec<String> {
    line.split(';')
        .next()
        .unwrap_or("")
        .replace(',', " ")
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}
pub fn assemble(source: &str, base: u32) -> Result<Assembly, String> {
    let mut symbols = BTreeMap::new();
    let mut pc = base;
    let mut high = base;
    for (line, text) in source.lines().enumerate() {
        let mut w = words(text);
        if w.is_empty() {
            continue;
        }
        if w[0].ends_with(':') {
            let name = w.remove(0);
            if symbols
                .insert(name.trim_end_matches(':').into(), pc)
                .is_some()
            {
                return Err(format!("line {}: duplicate label", line + 1));
            }
        }
        if w.is_empty() {
            continue;
        }
        match w[0].as_str() {
            ".equ" => {
                if w.len() != 3 {
                    return Err(format!("line {}: .equ name value", line + 1));
                }
                let v = value(&w[2], &symbols)?;
                symbols.insert(w[1].clone(), v);
            }
            ".org" => {
                if w.len() != 2 {
                    return Err(".org address".into());
                }
                pc = value(&w[1], &symbols)?;
                if pc < base {
                    return Err("origin below image base".into());
                }
            }
            ".word" => {
                pc = pc
                    .checked_add(4 * (w.len() - 1) as u32)
                    .ok_or("image overflow")?
            }
            ".byte" => {
                pc = pc
                    .checked_add((w.len() - 1) as u32)
                    .ok_or("image overflow")?
            }
            ".space" => {
                if w.len() != 2 {
                    return Err(".space count".into());
                }
                pc = pc
                    .checked_add(value(&w[1], &symbols)?)
                    .ok_or("image overflow")?;
            }
            _ => {
                if pc & 7 != 0 {
                    return Err(format!(
                        "line {}: instruction is not 8-byte aligned",
                        line + 1
                    ));
                }
                pc = pc.checked_add(8).ok_or("image overflow")?;
            }
        }
        high = high.max(pc);
        if high > 65536 {
            return Err("image exceeds address space".into());
        }
    }
    let mut bytes = vec![0xff; (high - base) as usize];
    pc = base;
    for (line, text) in source.lines().enumerate() {
        let mut w = words(text);
        if w.first().is_some_and(|s| s.ends_with(':')) {
            w.remove(0);
        }
        if w.is_empty() {
            continue;
        }
        let result: Result<(), String> = (|| {
            match w[0].as_str() {
                ".equ" => {}
                ".org" => pc = value(&w[1], &symbols)?,
                ".space" => pc += value(&w[1], &symbols)?,
                ".word" | ".byte" => {
                    for item in &w[1..] {
                        let n = value(item, &symbols)?;
                        let data = n.to_le_bytes();
                        let len = if w[0] == ".word" { 4 } else { 1 };
                        let i = (pc - base) as usize;
                        bytes[i..i + len].copy_from_slice(&data[..len]);
                        pc += len as u32;
                    }
                }
                _ => {
                    let ins = encode(&w, &symbols)?;
                    let i = (pc - base) as usize;
                    bytes[i..i + 8].copy_from_slice(&ins);
                    pc += 8;
                }
            }
            Ok(())
        })();
        result.map_err(|e| format!("line {}: {e}", line + 1))?;
    }
    Ok(Assembly {
        base,
        bytes,
        symbols,
    })
}
fn encode(w: &[String], symbols: &BTreeMap<String, u32>) -> Result<[u8; 8], String> {
    let names = [
        "nop", "movi", "mov", "add", "addi", "sub", "and", "or", "xor", "shl", "shr", "ldb", "ld",
        "stb", "st", "cmp", "cmpi", "jmp", "jz", "jnz", "jlo", "jhs", "call", "ret", "callr",
        "push", "pop", "wait", "ei", "di", "iret", "halt", "jmpr", "mul",
    ];
    let op = names
        .iter()
        .position(|n| *n == w[0])
        .ok_or_else(|| format!("unknown instruction {}", w[0]))?;
    let form = match op {
        0 | 23 | 27..=31 => 0,
        1 | 16 => 1,
        2 | 15 => 2,
        3 | 5..=8 | 33 => 3,
        4 | 9..=14 => 4,
        17..=22 => 5,
        24..=26 | 32 => 6,
        _ => unreachable!(),
    };
    let count = match form {
        0 => 1,
        1 | 2 => 3,
        3 | 4 => 4,
        _ => 2,
    };
    if w.len() != count {
        return Err(format!("{} expects {} operand(s)", w[0], count - 1));
    }
    let mut out = [0u8; 8];
    out[0] = op as u8;
    let mut imm = 0;
    if [1, 2, 3, 4, 6].contains(&form) {
        out[1] = reg(&w[1])?;
    }
    if [2, 3, 4].contains(&form) {
        out[2] = reg(&w[2])?;
    }
    if form == 3 {
        out[3] = reg(&w[3])?;
    }
    if form == 1 {
        imm = value(&w[2], symbols)?;
    }
    if form == 4 {
        imm = value(&w[3], symbols)?;
    }
    if form == 5 {
        imm = value(&w[1], symbols)?;
    }
    out[4..].copy_from_slice(&imm.to_le_bytes());
    Ok(out)
}
pub fn disassemble(bytes: &[u8], base: u32, count: usize) -> String {
    let mut out = String::new();
    let names = [
        "nop", "movi", "mov", "add", "addi", "sub", "and", "or", "xor", "shl", "shr", "ldb", "ld",
        "stb", "st", "cmp", "cmpi", "jmp", "jz", "jnz", "jlo", "jhs", "call", "ret", "callr",
        "push", "pop", "wait", "ei", "di", "iret", "halt", "jmpr", "mul",
    ];
    for (i, v) in bytes.chunks_exact(8).take(count).enumerate() {
        let op = v[0] as usize;
        let n = u32::from_le_bytes(v[4..8].try_into().unwrap());
        let s = if op >= names.len() || v[1..4].iter().any(|b| *b > 7) {
            format!(
                ".word 0x{:08x} 0x{n:08x}",
                u32::from_le_bytes(v[..4].try_into().unwrap())
            )
        } else {
            let args = match op {
                0 | 23 | 27..=31 => String::new(),
                1 | 16 => format!(" r{}, 0x{n:x}", v[1]),
                2 | 15 => format!(" r{}, r{}", v[1], v[2]),
                3 | 5..=8 | 33 => format!(" r{}, r{}, r{}", v[1], v[2], v[3]),
                4 | 9..=14 => format!(" r{}, r{}, 0x{n:x}", v[1], v[2]),
                17..=22 => format!(" 0x{n:x}"),
                _ => format!(" r{}", v[1]),
            };
            format!("{}{args}", names[op])
        };
        out.push_str(&format!("{:04x}  {}\n", base.wrapping_add(i as u32 * 8), s));
    }
    out
}
