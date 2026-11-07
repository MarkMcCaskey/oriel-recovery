# Ground console reference

Use `help` in either player for the current command list. Numeric values
accept decimal or `0x` hexadecimal. Hex byte strings contain pairs of hex
digits, without a `0x` prefix. Commands are case-sensitive.

| Task | Command |
| --- | --- |
| Mission handover | `briefing` |
| Predict contacts | `track`, `passes [COUNT]` |
| Advance time | `next`, `wait SECONDS` |
| Uplink rate | `rate 300` or `rate 1200` |
| Queue a command | `send ping`, `send read ADDRESS LENGTH`, `send write ADDRESS HEX`, `send service ID`, `send reset` |
| Update protocol | `send update begin`, `send update chunk HEX`, `send update commit` |
| Custom payload | `send hex HEX` |
| Exact framing | `frame HEX`, `raw HEX` |
| Upload workspace data | `upload FILE ADDRESS [FILE_OFFSET COUNT]` |
| Inspect/cancel queue | `queue`, `cancel` |
| Inspect received packets | `rx [COUNT]`, `decode [SEQUENCE or last]` |
| Raw communication log | `log [COUNT]` |
| Save response bytes | `extract SEQUENCE FILE` (omits response type byte) |
| Browse files | `files`, `cat FILE`, `hex FILE [OFFSET COUNT]` |
| Create local data | `put FILE HEX`, `write FILE TEXT` (`\n` makes a newline) |
| Assemble | `asm SOURCE BASE OUTPUT` |
| Disassemble | `disasm FILE BASE [OFFSET INSTRUCTION_COUNT]` |
| File checksum | `crc FILE` |
| Bench debugger | `lab help` |
| Investigation checkpoints | `snapshot NAME`, `restore NAME`, `snapshots` |
| Restart normal campaign | `new` |
| Native file import/export | `import PATH [NAME]`, `export NAME PATH` |
| Native exit | `quit` or Ctrl-D |

`next` advances to the next AOS plus five seconds; if already in contact,
it goes to the following pass. `wait` accepts fractions of a second, up to
86,400 seconds per invocation. Queued traffic needs a time advance to move.
Radio replies do not share uplink sequence numbers. An upload queues
48-byte RAM write commands; it does not guarantee delivery or acceptance.
`cancel` does not stop an in-flight frame or undo an executed command.

`lab load FILE ADDRESS`, `lab pc ADDRESS`, `lab step COUNT`, `lab run MS`,
`lab regs`, `lab mem ADDRESS LENGTH`, `lab break ADDRESS|off`, `lab inject HEX`
and `lab rx` operate on a separate local board. `lab reset` restores its
nominal archived image. `lab fault START END ADDRESS_AND DATA_AND DATA_OR
[inhibit]` adds a hypothesis; `lab clear-faults` removes hypotheses.

Browser Workbench: edit and assemble source, import/export workspace files,
and operate the local bench. Archive: read or download original engineering
files. Ground console: all flight interaction. Session exports are compatible
with native `--state PATH`; the mode stored in a session travels with it.
