use orbital_recovery::{
    asm,
    codec::{self, Reader, Writer},
    console::{self, Console},
    link::{self, Sim},
    machine::{Board, Fault},
    orbit,
};
fn board(src: &str) -> Board {
    let a = asm::assemble(src, 0).unwrap();
    Board::nominal(&a.bytes)
}
#[test]
fn arithmetic_and_memory() {
    let mut b=board("movi r0, 0xffffffff\nmovi r1, 2\nadd r2, r0, r1\nmovi r3, 0x8000\nst r2, r3, 3\nld r4, r3, 3\nsub r5, r1, r2\nmul r6, r1, r1\nhalt");
    b.run_cycles(100, None);
    assert_eq!(b.cpu.r[2], 1);
    assert_eq!(b.cpu.r[4], 1);
    assert_eq!(b.cpu.r[5], 1);
    assert_eq!(b.cpu.r[6], 4);
    assert!(b.cpu.carry);
    assert_eq!(&b.memory[0x8003..0x8007], &[1, 0, 0, 0]);
}
#[test]
fn bitwise_shift_and_move() {
    let mut b=board("movi r0, 0xf0\nmovi r1, 0x3c\nand r2, r0, r1\nor r3, r0, r1\nxor r4, r0, r1\nshl r5, r4, 36\nshr r6, r5, 4\nmov r0, r6\nhalt");
    b.run_cycles(30, None);
    assert_eq!(&b.cpu.r[..7], &[0xcc, 0x3c, 0x30, 0xfc, 0xcc, 0xcc0, 0xcc]);
}
#[test]
fn branches_calls_and_stack() {
    let mut b=board("movi r0, 0\ncall f\ncmpi r0, 1\njnz bad\njhs ok\nbad:\nhalt\nok:\nmovi r1, f\ncallr r1\ncmpi r0, 3\njlo low\nhalt\nlow:\nwait\nf:\naddi r0, r0, 1\nret");
    b.run_cycles(100, None);
    assert_eq!(b.cpu.r[0], 2);
    assert!(b.cpu.sleeping);
    assert_eq!(b.cpu.r[7], 0x9ff0);
}
#[test]
fn byte_access_and_register_stack() {
    let mut b=board("movi r0, 0x12345678\npush r0\nmovi r0, 0\npop r1\nmovi r2, 0x8000\nstb r1, r2, 0\nldb r3, r2, 0\nhalt");
    b.run_cycles(100, None);
    assert_eq!(b.cpu.r[1], 0x12345678);
    assert_eq!(b.cpu.r[3], 0x78);
}
#[test]
fn invalid_instructions_trap() {
    for bytes in [[255, 0, 0, 0, 0, 0, 0, 0], [1, 8, 0, 0, 0, 0, 0, 0]] {
        let mut b = Board::nominal(&bytes);
        b.step();
        assert!(b.cpu.trapped);
    }
    let mut b = Board::blank();
    b.cpu.pc = 1;
    b.step();
    assert!(b.cpu.trapped);
}
#[test]
fn interrupts_restore_context() {
    let mut b=board("jmp start\njmp handler\nstart:\nmovi r0, 77\nei\nwait\njmp start\nhandler:\nmovi r0, 12\nmovi r1, 0x8000\nst r0, r1, 0\niret");
    b.advance(1002, None);
    assert_eq!(b.memory[0x8000], 12);
    assert_eq!(b.cpu.r[0], 77);
    assert!(b.cpu.shadow.is_none());
}
#[test]
fn watchdog_clears_only_volatile_state() {
    let mut b = board("halt");
    b.memory[0x8000] = 7;
    b.memory[0xa000] = 9;
    b.advance(5001, None);
    assert_eq!(b.reset_count, 1);
    assert_eq!(b.memory[0x8000], 0);
    assert_eq!(b.memory[0xa000], 9);
    assert_eq!(b.reset_cause, 1);
}
#[test]
fn flash_timing_and_bit_rules() {
    let mut b = Board::blank();
    b.write32(0xf034, 0x1000);
    b.write32(0xf038, 0x12345678);
    b.write32(0xf03c, 0xc0de);
    b.write32(0xf030, 2);
    assert_eq!(b.read32(0x1000), u32::MAX);
    b.advance_devices(1);
    assert_eq!(b.read32(0xf030) & 1, 1);
    b.advance_devices(2);
    assert_eq!(b.read32(0x1000), 0x12345678);
    b.write32(0xf038, u32::MAX);
    b.write32(0xf03c, 0xc0de);
    b.write32(0xf030, 2);
    b.advance_devices(4);
    assert_eq!(b.read32(0x1000), 0x12345678);
    assert_eq!(b.flash_status, 2);
    b.write32(0xf03c, 0xc0de);
    b.write32(0xf030, 1);
    b.advance_devices(34);
    assert_eq!(b.read32(0x1000), u32::MAX);
    assert_eq!(b.wear[16], 1);
}
#[test]
fn flash_requires_key_and_protects_rom() {
    let mut b = Board::blank();
    b.write32(0xf034, 0x1000);
    b.write32(0xf030, 1);
    assert_eq!(b.flash_status, 2);
    b.write32(0xf034, 0);
    b.write32(0xf03c, 0xc0de);
    b.write32(0xf030, 1);
    assert!(b.flash_op.is_none());
    b.write32(0x1000, 0);
    assert_eq!(b.read32(0x1000), u32::MAX);
}
#[test]
fn interrupted_flash_is_not_atomic() {
    let mut b = Board::blank();
    b.memory[0x1000..0x1100].fill(0);
    b.write32(0xf034, 0x1000);
    b.write32(0xf03c, 0xc0de);
    b.write32(0xf030, 1);
    b.reset(2);
    assert!(b.memory[0x1000..0x1080].iter().all(|b| *b == 255));
    assert!(b.memory[0x1080..0x1100].iter().all(|b| *b == 0));
}
#[test]
fn configurable_bus_faults_are_consistent() {
    let mut b = Board::blank();
    b.faults.push(Fault {
        start: 0xa000,
        end: 0xb000,
        address_and: !0x100,
        data_and: 0xf0,
        data_or: 3,
        write_inhibit: false,
    });
    b.write8(0xa125, 0xab);
    assert_eq!(b.read8(0xa025), 0xa3);
    assert_eq!(b.read8(0xa125), 0xa3);
    assert_eq!(b.memory[0xa025], 0xa3);
    b.reset(2);
    assert_eq!(b.read8(0xa125), 0xa3);
}
#[test]
fn faults_can_inhibit_writes() {
    let mut b = Board::blank();
    b.faults.push(Fault {
        start: 0x8000,
        end: 0x8100,
        address_and: u32::MAX,
        data_and: 255,
        data_or: 0,
        write_inhibit: true,
    });
    b.write8(0x8000, 10);
    assert_eq!(b.read8(0x8000), 0);
    b.write8(0x8100, 10);
    assert_eq!(b.read8(0x8100), 10);
    assert_eq!(b.read8(0xd000), 255);
}
#[test]
fn crc_and_frame_validation() {
    assert_eq!(link::crc16(b"123456789"), 0x29b1);
    let f = link::frame(42, &[1, 2, 3]);
    assert_eq!(link::unframe(&f).unwrap(), (42, vec![1, 2, 3]));
    for i in 0..f.len() {
        let mut damaged = f.clone();
        damaged[i] ^= 1;
        assert!(link::unframe(&damaged).is_err());
    }
    assert_eq!(link::duration(104, 1200), 1000);
}
#[test]
fn orbit_has_finite_nonuniform_windows() {
    let ps = orbit::predictions(0, 6);
    for p in &ps {
        assert!(orbit::visible(p.aos));
        assert!(!orbit::visible(p.los));
        assert!(p.los - p.aos < 900000);
        assert!(p.los - p.aos > 1000);
    }
    assert!(ps
        .windows(2)
        .any(|w| (w[1].aos - w[0].aos).abs_diff(ps[1].aos - ps[0].aos) > 1000));
}
#[test]
fn no_contact_means_no_delivery() {
    let mut s = Sim::new(board("wait\njmp 0"));
    s.clean = true;
    s.queue(vec![1]);
    s.advance(s.pass.aos - 1);
    assert_eq!(s.queue.len(), 1);
    assert!(s.board.rx.is_empty());
    assert!(s.received.is_empty());
}
#[test]
fn contact_cutoff_preserves_only_completed_frames() {
    let mut s = Sim::new(board("wait\njmp 0"));
    s.clean = true;
    s.rate = 300;
    s.advance(s.pass.los - 20000);
    for _ in 0..40 {
        s.queue(vec![1; 100]);
    }
    s.advance(20000);
    let delivered = s.records.iter().filter(|r| r.state == "SENT").count();
    assert!(delivered > 0 && delivered < 10);
    assert!(s.records.iter().any(|r| r.state == "TRUNCATED"));
    assert!(s.queue.len() > 20);
}
#[test]
fn time_partitioning_is_deterministic() {
    let b = Board::nominal(include_bytes!("../archive/flight.bin"));
    let mut a = Sim::new(b.clone());
    let mut b = Sim::new(b);
    a.queue(vec![1]);
    b.queue(vec![1]);
    a.advance(400000);
    for _ in 0..400 {
        b.advance(1000);
    }
    assert_eq!(a, b);
}
#[test]
fn session_roundtrip_preserves_pending_traffic() {
    let mut c = Console::new(false).unwrap();
    c.exec("send ping");
    c.exec("next");
    c.exec("send read 0 128");
    c.exec("wait 0.1");
    let bytes = c.save();
    let mut d = Console::load(&bytes).unwrap();
    assert_eq!(c.exec("wait 70"), d.exec("wait 70"));
    assert_eq!(c.exec("log 4096"), d.exec("log 4096"));
    assert_eq!(c.save(), d.save());
}
#[test]
fn board_container_roundtrip() {
    let mut b = Board::nominal(include_bytes!("../archive/flight.bin"));
    b.advance(1234, None);
    let mut w = Writer(vec![]);
    codec::write_board(&mut w, &b);
    let sealed = codec::seal(&w.0);
    let bytes = codec::open(&sealed).unwrap();
    let mut r = Reader::new(&bytes);
    assert_eq!(b, codec::read_board(&mut r).unwrap());
    r.done().unwrap();
}
#[test]
fn containers_reject_corruption() {
    let c = Console::new(false).unwrap();
    let bytes = c.save();
    for n in [0, 1, 8, 16, 64, bytes.len() - 1] {
        assert!(Console::load(&bytes[..n]).is_err());
    }
    let mut bytes = bytes;
    bytes[200] ^= 1;
    assert!(Console::load(&bytes).is_err());
}
#[test]
fn public_console_has_no_live_debug_interface() {
    let mut c = Console::new(false).unwrap();
    assert!(c.exec("peek 0").starts_with("error:"));
    assert!(c.exec("repair").starts_with("error:"));
    assert!(c.exec("lab regs").contains("BENCH"));
}
#[test]
fn snapshots_restore_work_and_time() {
    let mut c = Console::new(false).unwrap();
    c.exec("write note.txt before");
    c.exec("snapshot a");
    let status = c.status_json();
    c.exec("wait 10");
    c.exec("write note.txt after");
    c.exec("restore a");
    assert_eq!(c.status_json(), status);
    assert_eq!(c.file("note.txt").unwrap(), b"before");
    let mut d = Console::load(&c.save()).unwrap();
    assert!(d.exec("snapshots").contains('a'));
}
#[test]
fn hardcore_disables_convenient_resets() {
    let mut c = Console::new(true).unwrap();
    for s in ["snapshot a", "restore a", "new"] {
        assert!(c.exec(s).starts_with("error:"));
    }
    assert!(Console::load(&c.save()).unwrap().hardcore());
}
#[test]
fn local_assembly_and_bench_example() {
    let mut c = Console::new(false).unwrap();
    assert!(c
        .exec("asm archive/bench-example.asm 0x8800 example.bin")
        .contains("Assembled"));
    c.exec("lab load example.bin 0x8800");
    c.exec("lab pc 0x8800");
    c.exec("lab step 65");
    assert!(c.exec("lab mem 0x8700 4").contains("37 00 00 00"));
}
#[test]
fn archive_fragments_match_linked_image() {
    for (name, base, offset) in [
        ("boot-main.asm", 0, 0),
        ("housekeeping.asm", 0x1000, 0x1000),
    ] {
        let text = std::fs::read_to_string(format!("archive/{name}")).unwrap();
        let a = asm::assemble(&text, base).unwrap();
        let flight = include_bytes!("../archive/flight.bin");
        assert_eq!(&a.bytes[..], &flight[offset..offset + a.bytes.len()]);
    }
}
#[test]
fn assembler_rejects_bad_input() {
    for s in [
        "movi r9, 1",
        ".org 999999999",
        "movi r0, missing",
        ".byte 1\nnop",
        "invalid",
        "a:\na:",
    ] {
        assert!(asm::assemble(s, 0).is_err(), "{s}");
    }
}
#[test]
fn standalone_encoder_roundtrip() {
    let src = "movi r0, 0xffffffff\naddi r1, r0, -4\nst r1, r0, 0x100\ncallr r3\niret";
    let a = asm::assemble(src, 0x8000).unwrap();
    let d = asm::disassemble(&a.bytes, 0x8000, 99);
    let plain = d
        .lines()
        .map(|l| l.split_once("  ").unwrap().1)
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(a.bytes, asm::assemble(&plain, 0x8000).unwrap().bytes);
}
#[test]
fn decoder_checks_record_integrity() {
    let mut p = vec![0x90];
    let words = [0x314b484fu32, 1234, 7600, 2200, 2, 12, 1, 0x103, 234];
    let sum = words.iter().fold(0u32, |s, v| s.wrapping_add(*v));
    for w in words {
        p.extend(w.to_le_bytes());
    }
    p.extend(sum.to_le_bytes());
    assert!(console::decode(&p).contains("ENGINEERING valid"));
    p[8] ^= 1;
    assert!(console::decode(&p).contains("invalid"));
}
#[test]
fn malformed_commands_do_not_panic() {
    let mut c = Console::new(false).unwrap();
    for s in [
        "send",
        "send write",
        "send read 0 99999",
        "asm",
        "upload",
        "raw xx",
        "wait NaN",
        "wait -1",
        "hex archive/flight.bin 999999",
        "put archive/x 00",
        "lab load missing 0",
        "write",
        "frame ",
    ] {
        c.exec(s);
    }
    assert!(!c.status_json().is_empty());
}

#[test]
fn radio_emissions_outside_visibility_are_not_observable() {
    let mut s = Sim::new(board("wait\njmp 0"));
    s.clean = true;
    s.board.tx.push_back(vec![0x81, 7, 8, 9]);
    s.advance(1000);
    assert!(s.board.tx.is_empty());
    assert!(s.downlink.is_none());
    assert!(s.received.is_empty());
    assert!(s.records.is_empty());
}

#[test]
fn downlink_continues_transmitting_beyond_ground_horizon() {
    let mut s = Sim::new(board("wait\njmp 0"));
    s.clean = true;
    let los = s.pass.los;
    s.advance(los - 100);
    s.board.tx.push_back(vec![0x81; 256]);
    s.advance(100);
    assert!(s.downlink.is_some());
    assert!(s
        .records
        .iter()
        .any(|r| r.direction == 1 && r.state == "TRUNCATED" && r.wire.len() < 264));
    s.advance(2000);
    assert!(s.downlink.is_none());
    assert!(!s.received.iter().any(|(_, _, p)| p.len() == 256));
}

#[test]
fn ground_logs_do_not_claim_unobservable_uplink_acceptance() {
    let mut s = Sim::new(board("wait\njmp 0"));
    s.clean = true;
    s.advance(s.pass.aos + 5000);
    s.queue.push_back(vec![0; 30]);
    s.advance(1000);
    assert!(s.board.rx.is_empty());
    assert!(s
        .records
        .iter()
        .any(|r| r.direction == 0 && r.state == "SENT"));
    assert!(!s.records.iter().any(|r| r.state.contains("REJECTED")));
}

#[test]
fn interrupted_downlink_roundtrips_at_loss_of_signal() {
    let mut a = Sim::new(board("wait\njmp 0"));
    a.clean = true;
    let los = a.pass.los;
    a.advance(los - 100);
    a.board.tx.push_back(vec![0x81; 256]);
    a.advance(100);
    let mut w = Writer(vec![]);
    codec::write_sim(&mut w, &a);
    let mut b = codec::read_sim(&mut Reader::new(&w.0)).unwrap();
    a.advance(2000);
    b.advance(2000);
    assert_eq!(a, b);
}

#[test]
fn empty_radio_payload_is_rejected_by_controller() {
    assert!(link::unframe(&link::frame(1, &[])).is_err());
}
