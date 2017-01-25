extern crate clap;

use std::cmp;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::str;
use clap::{App, Arg};

struct Memory {
    memory: Vec<u8>,
    stack: Vec<u16>,
}

impl Memory {
    fn new(buffer: Vec<u8>) -> Memory {
        let stack: Vec<u16> = Vec::new();
        Memory {
            memory: buffer,
            stack: stack,
        }
    }

    fn len(&self) -> usize {
        self.memory.len()
    }

    fn read_u8(&self, offset: usize) -> u8 {
        self.memory[offset]
    }

    fn read_u16(&self, offset: usize) -> u16 {
        ((self.memory[offset] as u16) << 8) | (self.memory[offset + 1] as u16)
    }

    fn write_u8(&mut self, offset: usize, val: u8) {
        self.memory[offset] = val
    }

    fn write_u16(&mut self, offset: usize, val: u16) {
        self.memory[offset] = (val >> 8) as u8;
        self.memory[offset + 1] = (val & 0xff) as u8;
    }
}

#[derive(Debug, Copy, Clone)]
struct Header {
    dynamic_start: usize,
    dynamic_end: usize,
    static_start: usize,
    static_end: usize,
    high_start: usize,
    high_end: usize,
}

impl Header {
    fn new(mem: &Memory) -> Header {
        let dynamic_start = 0;
        let dynamic_end = mem.read_u16(0xe) as usize;
        let static_start = dynamic_end;
        let static_end = static_start + cmp::min(0xffff, mem.len());
        let high_start = mem.read_u16(0x4) as usize;
        let high_end = mem.len();

        Header {
            dynamic_start: dynamic_start,
            dynamic_end: dynamic_end,
            static_start: static_start,
            static_end: static_end,
            high_start: high_start,
            high_end: high_end,
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum Operand {
    Large(u16),
    Small(u8),
    Variable(u8),
    Omitted,
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Operand::Large(x) => write!(f, "#{:04x}", x),
            &Operand::Small(x) => write!(f, "#{:02x}", x),
            &Operand::Variable(x) if x == 0 => write!(f, "(SP)+"),
            &Operand::Variable(x) if x >= 0x10 => write!(f, "G{:02x}", x - 0x10),
            &Operand::Variable(x) => write!(f, "L{:02x}", x - 1),
            &Operand::Omitted => write!(f, ""),
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum Return {
    Variable(u8),
    Omitted,
}

impl fmt::Display for Return {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Return::Variable(x) if x == 0 => write!(f, " -> -(SP)"),
            &Return::Variable(x) if x >= 0x10 => write!(f, " -> G{:02x}", x - 0x10),
            &Return::Variable(x) => write!(f, " -> L{:02x}", x - 1),
            _ => write!(f, ""),
        }
    }
}

#[derive(Debug)]
struct ZString {
    offset: usize,
    length: usize,
    contents: String,
}

enum ZStringShift {
    Zero,
    One,
    Two,
}

enum ZStringState {
    GetNext(u8),
    GetNextNext(u8, u8),
    GetNothing,
}

impl ZString {
    fn new(memory: &Memory, offset: usize) -> ZString {
        let mut length = 0usize;
        let mut bytes: Vec<u8> = Vec::new();
        loop {
            let x = memory.read_u16(offset + length);
            length += 2;

            bytes.push(((x >> 10) & 0x1f) as u8);
            bytes.push(((x >> 5) & 0x1f) as u8);
            bytes.push((x & 0x1f) as u8);

            if (x & 0x8000) != 0 {
                break;
            }
        }

        let mut shift = ZStringShift::Zero;
        let mut state = ZStringState::GetNothing;
        let contents = bytes.into_iter().fold(String::new(), |c, x| {
            let enable_utf = if let ZStringShift::Two = shift {
                true
            } else {
                false
            };
            match state {
                ZStringState::GetNothing => {
                    match x {
                        0 => c + " ",
                        1 | 2 | 3 => {
                            state = ZStringState::GetNext(x);
                            c
                        }
                        4 => {
                            shift = ZStringShift::One;
                            c
                        }
                        5 => {
                            shift = ZStringShift::Two;
                            c
                        }
                        6 if enable_utf => {
                            state = ZStringState::GetNext(x);
                            c
                        }
                        a if a > 5 && a < 32 => {
                            let alphabet = match shift {
                                ZStringShift::Zero => "______abcdefghijklmnopqrstuvwxyz",
                                ZStringShift::One => "______ABCDEFGHIJKLMNOPQRSTUVWXYZ",
                                ZStringShift::Two => "______^\n0123456789.,!?_#\'\"/\\-:()",
                            };
                            shift = ZStringShift::Zero;
                            c + &alphabet.chars().nth(a as usize).unwrap().to_string()
                        }
                        _ => c,
                    }
                }
                ZStringState::GetNext(a) if a > 0 && a < 4 => {
                    state = ZStringState::GetNothing;
                    let table = memory.read_u16(0x18) as usize;
                    let index = (32 * (a - 1) + x) as usize;
                    let offset = memory.read_u16(table + index * 2) as usize;
                    let abbrev = ZString::new(memory, offset * 2);
                    c + &abbrev.contents
                }
                ZStringState::GetNext(6) => {
                    state = ZStringState::GetNextNext(6, x);
                    c
                }
                ZStringState::GetNextNext(6, b) => {
                    state = ZStringState::GetNothing;
                    let utf_char = (b << 5) | x;
                    let v = vec![utf_char];
                    c + str::from_utf8(&v).unwrap()
                }
                _ => c,
            }
        });

        ZString {
            offset: offset,
            length: length,
            contents: contents,
        }
    }
}

impl fmt::Display for ZString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.contents)
    }
}

#[derive(Debug, Copy, Clone)]
enum Encoding {
    Op0,
    Op1,
    Op2,
    Var,
}

#[derive(Debug)]
struct Instruction {
    offset: usize,
    opcode: usize,
    optype: Encoding,
    length: usize,
    args: Vec<Operand>,
    ret: Return,
    string: Option<ZString>,
    jump_offset: Option<i32>,
    compare: Option<bool>,
}

impl Instruction {
    fn name(&self) -> &str {
        match self.optype {
            Encoding::Op0 => {
                match self.opcode {
                    0 => "rtrue",
                    1 => "rfalse",
                    2 => "print",
                    3 => "print_ret",
                    4 => "no",
                    5 => "save",
                    6 => "restore",
                    7 => "restart",
                    8 => "ret_popped",
                    9 => "pop",
                    10 => "quit",
                    11 => "new_line",
                    12 => "show_status",
                    13 => "verify",
                    14 => "extended",
                    15 => "piracy",
                    _ => "unknown",
                }
            }
            Encoding::Op1 => {
                match self.opcode {
                    0 => "jz",
                    1 => "get_sibling",
                    2 => "get_child",
                    3 => "get_parent",
                    4 => "get_prop_len",
                    5 => "inc",
                    6 => "dec",
                    7 => "print_addr",
                    8 => "call_1s",
                    9 => "remove_obj",
                    10 => "print_obj",
                    11 => "ret",
                    12 => "jump",
                    13 => "print_paddr",
                    14 => "load",
                    15 => "not",
                    16 => "call_1n",
                    _ => "unknown",
                }
            }
            Encoding::Op2 => {
                match self.opcode {
                    0 => "none",
                    1 => "je",
                    2 => "jl",
                    3 => "jg",
                    4 => "dec_chk",
                    5 => "inc_chk",
                    6 => "jin",
                    7 => "test",
                    8 => "or",
                    9 => "and",
                    10 => "test_attr",
                    11 => "set_attr",
                    12 => "clear_attr",
                    13 => "store",
                    14 => "insert_obj",
                    15 => "loadw",
                    16 => "loadb",
                    17 => "get_prop",
                    18 => "get_prop_addr",
                    19 => "get_next_prop",
                    20 => "add",
                    21 => "sub",
                    22 => "mul",
                    23 => "div",
                    24 => "mod",
                    25 => "call_2s",
                    26 => "call_2n",
                    27 => "set_colour",
                    _ => "unknown",
                }
            }
            Encoding::Var => {
                match self.opcode {
                    0 => "call",
                    1 => "storew",
                    2 => "storeb",
                    3 => "put_prop",
                    4 => "sread",
                    5 => "print_char",
                    6 => "print_num",
                    7 => "random",
                    8 => "push",
                    9 => "pull",
                    10 => "split_window",
                    11 => "set_window",
                    12 => "call-vs2",
                    13 => "erase_window",
                    14 => "erase_line",
                    15 => "set_cursor",
                    16 => "get_cursor",
                    17 => "set_text_style",
                    18 => "buffer_mode",
                    19 => "output_stream",
                    20 => "input_stream",
                    21 => "sound_effect",
                    22 => "read_char",
                    23 => "scan_table",
                    24 => "not_v4",
                    25 => "call_vn",
                    26 => "call_vn2",
                    27 => "tokenise",
                    28 => "encode_text",
                    29 => "copy_table",
                    30 => "print_table",
                    31 => "check_arg_count",
                    _ => "unknown",
                }
            }
        }
    }

    fn decode_short(memory: &Memory, offset: usize, op: u8) -> Instruction {
        let (optype, length, args) = match (op & 0x30) >> 4 {
            3 => (Encoding::Op0, 1, Vec::new()),
            2 => (Encoding::Op1, 2, vec![Operand::Variable(memory.read_u8(offset + 1))]),
            1 => (Encoding::Op1, 2, vec![Operand::Small(memory.read_u8(offset + 1))]),
            _ => (Encoding::Op1, 3, vec![Operand::Large(memory.read_u16(offset + 1))]),
        };
        Instruction {
            offset: offset,
            opcode: (op & 0xf) as usize,
            optype: optype,
            length: length,
            args: args,
            ret: Return::Omitted,
            string: None,
            jump_offset: None,
            compare: None,
        }
    }

    fn decode_long(memory: &Memory, offset: usize, op: u8) -> Instruction {
        let x = memory.read_u8(offset + 1);
        let y = memory.read_u8(offset + 2);
        Instruction {
            offset: offset,
            opcode: (op & 0x1f) as usize,
            optype: Encoding::Op2,
            length: 3,
            args: vec![if (op & 0x40) != 0 {
                           Operand::Variable(x)
                       } else {
                           Operand::Small(x)
                       },
                       if (op & 0x20) != 0 {
                           Operand::Variable(y)
                       } else {
                           Operand::Small(y)
                       }],
            ret: Return::Omitted,
            string: None,
            jump_offset: None,
            compare: None,
        }
    }

    fn decode_var(memory: &Memory, offset: usize, op: u8) -> Instruction {
        let optypes = memory.read_u8(offset + 1);
        let mut size = 2;
        let mut args: Vec<Operand> = Vec::new();
        for x in 0..3 {
            let shift = (3 - x) * 2;
            let mask = 3 << shift;
            args.push(match (optypes & mask) >> shift {
                3 => Operand::Omitted,
                2 => {
                    size += 1;
                    Operand::Variable(memory.read_u8(offset + size - 1))
                }
                1 => {
                    size += 1;
                    Operand::Small(memory.read_u8(offset + size - 1))
                }
                _ => {
                    size += 2;
                    Operand::Large(memory.read_u16(offset + size - 2))
                }
            });
        }
        Instruction {
            offset: offset,
            opcode: (op & 0x1f) as usize,
            optype: if (op & 0x20) != 0 {
                Encoding::Var
            } else {
                Encoding::Op2
            },
            length: size,
            args: args.into_iter()
                .filter(|x| if let &Operand::Omitted = x {
                    false
                } else {
                    true
                })
                .collect(),
            ret: Return::Omitted,
            string: None,
            jump_offset: None,
            compare: None,
        }
    }

    fn add_return(&mut self, memory: &Memory) {
        if match self.optype {
            Encoding::Op2 => {
                (self.opcode >= 0x08 && self.opcode <= 0x09) ||
                (self.opcode >= 0x0f && self.opcode <= 0x19)
            }
            Encoding::Op1 => {
                (self.opcode >= 0x01 && self.opcode <= 0x04) || self.opcode == 0x08 ||
                (self.opcode >= 0x0e && self.opcode <= 0x0f)
            }
            Encoding::Var => self.opcode == 0x0,
            _ => false,
        } {
            self.ret = Return::Variable(memory.read_u8(self.offset + self.length));
            self.length += 1;
        }
    }

    fn add_branch(&mut self, memory: &Memory) {
        if match self.optype {
            Encoding::Op2 => (self.opcode >= 1 && self.opcode <= 7) || (self.opcode == 10),
            Encoding::Op1 => self.opcode <= 2,
            Encoding::Var => {
                self.opcode == 5 || self.opcode == 6 || self.opcode == 0xd || self.opcode == 0xf
            }
            _ => false,
        } {
            let branch1 = memory.read_u8(self.offset + self.length) as i32;
            let mut offset = (0x80 & branch1) << 8;
            let len: usize;
            if (branch1 & 0x40) != 0 {
                offset = offset | (branch1 & 0x3f);
                len = 1;
            } else {
                offset = offset | ((branch1 & 0x1f) << 8) |
                         (memory.read_u8(self.offset + self.length + 1) as i32);
                len = 2;
            }
            let compare = (offset & 0x8000) != 0;
            offset = offset & 0x7fff;
            self.jump_offset = Some(offset);
            self.length = self.length + len;
            self.compare = Some(compare);
        }
    }

    fn add_print(&mut self, memory: &Memory) {
        if match self.optype {
            Encoding::Op0 => self.opcode == 2 || self.opcode == 3,
            _ => false,
        } {
            let s = ZString::new(memory, self.offset + self.length);
            self.length += s.length;
            self.string = Some(s);
        }
    }

    fn new(memory: &Memory, offset: usize) -> Instruction {
        let op = memory.read_u8(offset);
        let mut i = match (op & 0xc0) >> 6 {
            3 => Instruction::decode_var(memory, offset, op),
            2 => Instruction::decode_short(memory, offset, op),
            _ => Instruction::decode_long(memory, offset, op),
        };
        i.add_return(memory);
        i.add_branch(memory);
        i.add_print(memory);
        i
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let args: Vec<String> = self.args.iter().map(|a| format!("{}", a)).collect();
        if let Some(ref x) = self.string {
            write!(f,
                   "[{:08X}] {}\t{}{}\"{}\"",
                   self.offset,
                   self.name().to_uppercase(),
                   args.join(","),
                   self.ret,
                   x)
        } else {
            write!(f,
                   "[{:08X}] {}\t{}{}",
                   self.offset,
                   self.name().to_uppercase(),
                   args.join(","),
                   self.ret)
        }
    }
}

struct Machine {
    memory: Memory,
    header: Header,
    ip: usize,
    finished: bool,
}

impl Machine {
    fn new(memory: Memory, header: Header) -> Machine {
        Machine {
            ip: memory.read_u16(0x6) as usize,
            memory: memory,
            header: header,
            finished: false,
        }
    }

    fn decode(&self) -> Instruction {
        Instruction::new(&self.memory, self.ip)
    }

    fn execute(&mut self, i: Instruction) -> Result<(), String> {
        let oldip = self.ip;
        match i.name() {
            _ => return Err(format!("unimplemented instruction:\n{}", i)),
        }
        if self.ip == oldip {
            self.ip += i.length;
        }
        Ok(())
    }
}

fn open_z3(filename: &str) -> Result<Machine, std::io::Error> {
    let mut file = try!(File::open(filename));
    let mut buffer: Vec<u8> = Vec::new();
    try!(file.read_to_end(&mut buffer));

    let memory = Memory::new(buffer);
    let header = Header::new(&memory);

    Ok(Machine::new(memory, header))
}

fn main() {
    let matches = App::new("rustzork")
        .version("1.0")
        .about("Interpreter for V3 zmachine spec.")
        .arg(Arg::with_name("file").help("Path to the .z3 file to run").index(1).required(true))
        .get_matches();

    let filename = matches.value_of("file").unwrap();
    let mut machine = match open_z3(filename) {
        Ok(x) => x,
        Err(e) => {
            println!("Error opening file: {}", e);
            std::process::exit(1);
        }
    };

    while !machine.finished {
        let i = machine.decode();
        println!("{}", i);
        if let Err(e) = machine.execute(i) {
            println!("{}", e);
            break;
        }
    }
}
