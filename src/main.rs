use orbital_recovery::{asm, console::BRIEFING, Console};
use std::{
    env, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};
fn persist(c: &Console, path: &Path) -> io::Result<()> {
    if let Some(p) = path.parent() {
        if !p.as_os_str().is_empty() {
            fs::create_dir_all(p)?;
        }
    }
    let tmp = path.with_extension("session.tmp");
    fs::write(&tmp, c.save())?;
    fs::rename(tmp, path)
}
fn main() {
    if let Err(e) = run() {
        eprintln!("ground: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--assemble") {
        if args.len() != 4 {
            return Err("usage: ground --assemble SOURCE BASE OUTPUT".into());
        }
        let a = asm::assemble(&fs::read_to_string(&args[1])?, asm::number(&args[2])?)?;
        fs::write(&args[3], &a.bytes)?;
        let symbols = a
            .symbols
            .iter()
            .map(|(n, v)| format!("{v:08x} {n}\n"))
            .collect::<String>();
        fs::write(format!("{}.sym", args[3]), symbols)?;
        println!("{} bytes assembled.", a.bytes.len());
        return Ok(());
    }
    if args.iter().any(|s| s == "--help") {
        println!("ground [--state PATH] [--hardcore] [--script PATH]\nground --assemble SOURCE BASE OUTPUT\n\nDefault session: .local/station.session; saved after every command.\nType help inside the console. Browser and CLI session files are interchangeable.");
        return Ok(());
    }
    let mut state = PathBuf::from(".local/station.session");
    let mut script = None;
    let mut hardcore = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--state" => {
                i += 1;
                state = PathBuf::from(args.get(i).ok_or("--state needs a path")?);
            }
            "--script" => {
                i += 1;
                script = Some(PathBuf::from(args.get(i).ok_or("--script needs a path")?));
            }
            "--hardcore" => hardcore = true,
            _ => return Err(format!("unknown option {}", args[i]).into()),
        }
        i += 1;
    }
    let mut c = if state.exists() {
        Console::load(&fs::read(&state)?)?
    } else {
        Console::new(hardcore)?
    };
    if hardcore {
        c.enable_hardcore();
    }
    println!("{BRIEFING}");
    println!("{}", c.exec("track"));
    let scripted = script.is_some();
    let input: Box<dyn BufRead> = if let Some(p) = script {
        Box::new(io::BufReader::new(fs::File::open(p)?))
    } else {
        Box::new(io::BufReader::new(io::stdin()))
    };
    persist(&c, &state)?;
    if !scripted {
        print!("ground> ");
        io::stdout().flush()?;
    }
    for line in input.lines() {
        let line = line?;
        let w: Vec<_> = line.split_whitespace().collect();
        if w.first() == Some(&"quit") || w.first() == Some(&"exit") {
            break;
        }
        if line.trim_start().starts_with('#') {
            continue;
        }
        if scripted {
            println!("ground> {line}");
        }
        let result: Result<String, Box<dyn std::error::Error>> = (|| match w.first().copied() {
            Some("import") => {
                let path = w.get(1).ok_or("import PATH [NAME]")?;
                let name = w.get(2).copied().unwrap_or_else(|| {
                    Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("input.bin")
                });
                c.put(name, fs::read(path)?)?;
                Ok(format!("Imported {name}.\n"))
            }
            Some("export") => {
                let name = w.get(1).ok_or("export NAME PATH")?;
                let path = w.get(2).ok_or("export NAME PATH")?;
                fs::write(path, c.file(name)?)?;
                Ok(format!("Exported {name}.\n"))
            }
            _ => Ok(c.exec(&line)),
        })();
        match result {
            Ok(out) => print!("{out}"),
            Err(e) => println!("error: {e}"),
        };
        persist(&c, &state)?;
        if !scripted {
            print!("ground> ");
            io::stdout().flush()?;
        }
    }
    Ok(())
}
