#[cfg(feature = "cli")]
extern crate clap;
extern crate rand;

use rand::rngs::SmallRng;
use rand::{FromEntropy, RngCore, SeedableRng};
use std::cmp;
use std::fmt;
use std::str;

#[derive(Debug, Copy, Clone)]
enum Operand {
    Large(u16),
    Small(u8),
    Variable(u8),
    Indirect(u8),
    Omitted,
}

#[cfg(not(feature = "cli"))]
extern "C" {
    fn clear();
    #[allow(dead_code)]
    fn debug_trace(x: i32);
    fn terminal_height() -> i32;
    fn put_line(x: i32, y: i32, text: *const u8, len: i32);
}

#[cfg(not(feature = "cli"))]
enum InputState {
    None,
    Listening,
    Consuming,
}

#[cfg(not(feature = "cli"))]
struct ZIO {
    buffer: String,
    input: String,
    flushed: bool,
    state: InputState,
}

#[cfg(not(feature = "cli"))]
impl ZIO {
    fn new() -> ZIO {
        ZIO {
            buffer: String::new(),
            input: String::new(),
            flushed: true,
            state: InputState::None,
        }
    }
    fn print(&mut self, s: &str) -> () {
        if s.ends_with("n") {
            self.flushed = false;
        }
        self.buffer += s;
    }
    fn flush(&mut self) -> Result<(), std::io::Error> {
        self.flushed = false;
        Ok(())
    }
    fn log(&mut self, s: &str) -> () {
        self.buffer += s;
        self.buffer += "\n";
        self.flushed = false;
    }

    fn key_down(&mut self, key: u8) {
        if let InputState::Listening = self.state {
            if key == 13 {
                self.buffer.push('\n');
                self.state = InputState::Consuming;
            } else {
                self.buffer.push(key as char);
                self.input.push(key as char);
            }
            self.flushed = false;
        }
    }

    fn poll_input(&mut self) -> bool {
        if let InputState::Consuming = self.state {
            true
        } else {
            if let InputState::None = self.state {
                self.buffer.push(' ');
                self.state = InputState::Listening;
                self.input = String::new();
            }
            false
        }
    }

    fn input(&mut self) -> String {
        self.state = InputState::None;
        self.input.clone()
    }
    fn draw(&mut self) -> () {
        if !self.flushed {
            self.flushed = true;
            unsafe {
                clear();
            }
            let max_lines = unsafe { terminal_height() } as usize;
            let lines: Vec<_> = self.buffer.lines().collect();
            let start = if lines.len() > max_lines {
                lines.len() - max_lines
            } else {
                0
            };
            for (y, l) in lines[start..].iter().enumerate() {
                unsafe {
                    put_line(0, y as i32, l.as_ptr(), l.len() as i32);
                }
            }
        }
    }
}

#[cfg(feature = "cli")]
struct ZIO {
    input: String,
}

#[cfg(feature = "cli")]
impl ZIO {
    fn new() -> ZIO {
        ZIO {
            input: String::new(),
        }
    }
    fn print(&mut self, s: &str) -> () {
        print!("{}", s);
    }
    fn flush(&mut self) -> Result<(), std::io::Error> {
        use std::io::Write;
        std::io::stdout().flush()
    }
    fn log(&mut self, s: &str) -> () {
        println!("{}", s);
    }
    fn poll_input(&mut self) -> bool {
        self.input = String::new();
        let stdin = std::io::stdin();
        if let Ok(_) = stdin.read_line(&mut self.input) {
            true
        } else {
            false
        }
    }
    fn input(&self) -> String {
        self.input.clone()
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Operand::Large(x) => write!(f, "#{:04x}", x),
            Operand::Small(x) => write!(f, "#{:02x}", x),
            Operand::Variable(x) if x == 0 => write!(f, "(SP)+"),
            Operand::Variable(x) if x >= 0x10 => write!(f, "G{:02x}", x - 0x10),
            Operand::Variable(x) => write!(f, "L{:02x}", x - 1),
            Operand::Indirect(x) if x == 0 => write!(f, "[(SP)]"),
            Operand::Indirect(x) if x >= 0x10 => write!(f, "[G{:02x}]", x - 0x10),
            Operand::Indirect(x) => write!(f, "[L{:02x}]", x - 1),
            Operand::Omitted => write!(f, ""),
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum Return {
    Variable(u8),
    Indirect(u8),
    Omitted,
}

impl fmt::Display for Return {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Return::Variable(x) if x == 0 => write!(f, " -> -(SP)"),
            &Return::Variable(x) if x >= 0x10 => write!(f, " -> G{:02x}", x - 0x10),
            &Return::Variable(x) => write!(f, " -> L{:02x}", x - 1),
            &Return::Indirect(x) if x == 0 => write!(f, " -> (SP)"),
            &Return::Indirect(x) if x >= 0x10 => write!(f, " -> G{:02x}", x - 0x10),
            &Return::Indirect(x) => write!(f, " -> L{:02x}", x - 1),
            _ => write!(f, ""),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    addr: usize,
    stack_start: usize,
    num_locals: usize,
    return_storage: Return,
    return_addr: usize,
}

struct Memory {
    memory: Vec<u8>,
    stack: Vec<u16>,
    frames: Vec<Frame>,
}

impl Memory {
    fn new(buffer: &[u8]) -> Memory {
        Memory {
            memory: Vec::from(buffer),
            stack: Vec::new(),
            frames: Vec::new(),
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

#[derive(Debug, Clone)]
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

impl ZString {
    fn with_bytes(memory: &Memory, offset: usize, length: usize, bytes: &[u8]) -> ZString {
        let mut shift = ZStringShift::Zero;
        let mut contents = String::new();
        let mut it = bytes.into_iter();
        while let Some(c) = it.next() {
            match *c {
                0 => contents.push(' '),
                1 | 2 | 3 => {
                    let offset = *c as usize;
                    let abbrev = *it.next().unwrap() as usize;
                    let table = memory.read_u16(0x18) as usize;
                    let index = 32 * (offset - 1) + abbrev;
                    let offset = memory.read_u16(table + index * 2) as usize;
                    let abbrev = ZString::new(memory, offset * 2);
                    contents += &abbrev.contents;
                }
                4 => shift = ZStringShift::One,
                5 => shift = ZStringShift::Two,
                _ => {
                    match shift {
                        ZStringShift::Two if *c == 6 => {
                            let mut utf_char = it.next().unwrap() << 5;
                            utf_char |= it.next().unwrap() & 0x1f;
                            contents += str::from_utf8(&[utf_char]).unwrap();
                        }
                        _ => {
                            let alphabet = match shift {
                                ZStringShift::Zero => "______abcdefghijklmnopqrstuvwxyz",
                                ZStringShift::One => "______ABCDEFGHIJKLMNOPQRSTUVWXYZ",
                                ZStringShift::Two => "______^\n0123456789.,!?_#\'\"/\\-:()",
                            };
                            contents += &alphabet.chars().nth(*c as usize).unwrap().to_string()
                        }
                    }
                    shift = ZStringShift::Zero;
                }
            }
        }

        ZString {
            offset: offset,
            length: length,
            contents: contents,
        }
    }

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
        ZString::with_bytes(memory, offset, length, &bytes)
    }

    fn with_max_length(memory: &Memory, offset: usize, max_length: usize) -> ZString {
        let mut length = 0usize;
        let mut bytes: Vec<u8> = Vec::new();
        loop {
            if length == max_length {
                break;
            }
            let x = memory.read_u16(offset + length);
            length += 2;

            bytes.push(((x >> 10) & 0x1f) as u8);
            bytes.push(((x >> 5) & 0x1f) as u8);
            bytes.push((x & 0x1f) as u8);

            if (x & 0x8000) != 0 {
                break;
            }
        }
        ZString::with_bytes(memory, offset, length, &bytes)
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
        let names0op = [
            "rtrue",
            "rfalse",
            "print",
            "print_ret",
            "no",
            "save",
            "restore",
            "restart",
            "ret_popped",
            "pop",
            "quit",
            "new_line",
            "show_status",
            "verify",
            "extended",
            "piracy",
        ];
        let names1op = [
            "jz",
            "get_sibling",
            "get_child",
            "get_parent",
            "get_prop_len",
            "inc",
            "dec",
            "print_addr",
            "call_1s",
            "remove_obj",
            "print_obj",
            "ret",
            "jump",
            "print_paddr",
            "load",
            "not",
            "call_1n",
        ];
        let names2op = [
            "none",
            "je",
            "jl",
            "jg",
            "dec_chk",
            "inc_chk",
            "jin",
            "test",
            "or",
            "and",
            "test_attr",
            "set_attr",
            "clear_attr",
            "store",
            "insert_obj",
            "loadw",
            "loadb",
            "get_prop",
            "get_prop_addr",
            "get_next_prop",
            "add",
            "sub",
            "mul",
            "div",
            "mod",
            "call_2s",
            "call_2n",
            "set_colour",
            "throw",
        ];
        let namesvar = [
            "call",
            "storew",
            "storeb",
            "put_prop",
            "sread",
            "print_char",
            "print_num",
            "random",
            "push",
            "pull",
            "split_window",
            "set_window",
            "call_vs2",
            "erase_window",
            "erase_line",
            "set_cursor",
            "get_cursor",
            "set_text_style",
            "buffer_mode",
            "output_stream",
            "input_stream",
            "sound_effect",
            "read_char",
            "scan_table",
            "not_v4",
            "call_vn",
            "call_vn2",
            "tokenise",
            "encode_text",
            "copy_table",
            "print_table",
            "check_arg_count",
        ];

        match self.optype {
            Encoding::Op0 => names0op.get(self.opcode).unwrap_or(&"unknown"),
            Encoding::Op1 => names1op.get(self.opcode).unwrap_or(&"unknown"),

            Encoding::Op2 => names2op.get(self.opcode).unwrap_or(&"unknown"),

            Encoding::Var => namesvar.get(self.opcode).unwrap_or(&"unknown"),
        }
    }

    fn decode_short(memory: &Memory, offset: usize, op: u8) -> Instruction {
        let (optype, length, args) = match (op & 0x30) >> 4 {
            3 => (Encoding::Op0, 1, Vec::new()),
            2 => (
                Encoding::Op1,
                2,
                vec![Operand::Variable(memory.read_u8(offset + 1))],
            ),
            1 => (
                Encoding::Op1,
                2,
                vec![Operand::Small(memory.read_u8(offset + 1))],
            ),
            _ => (
                Encoding::Op1,
                3,
                vec![Operand::Large(memory.read_u16(offset + 1))],
            ),
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
            args: vec![
                if (op & 0x40) != 0 {
                    Operand::Variable(x)
                } else {
                    Operand::Small(x)
                },
                if (op & 0x20) != 0 {
                    Operand::Variable(y)
                } else {
                    Operand::Small(y)
                },
            ],
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
        for x in 0..4 {
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
            args: args
                .into_iter()
                .filter(|x| {
                    if let &Operand::Omitted = x {
                        false
                    } else {
                        true
                    }
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
                (self.opcode >= 0x08 && self.opcode <= 0x09)
                    || (self.opcode >= 0x0f && self.opcode <= 0x19)
            }
            Encoding::Op1 => {
                (self.opcode >= 0x01 && self.opcode <= 0x04)
                    || self.opcode == 0x08
                    || (self.opcode >= 0x0e && self.opcode <= 0x0f)
            }
            Encoding::Var => self.opcode == 0x0 || self.opcode == 0x7,
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
            Encoding::Op0 => {
                self.opcode == 5 || self.opcode == 6 || self.opcode == 0xd || self.opcode == 0xf
            }
            _ => false,
        } {
            let branch1 = memory.read_u8(self.offset + self.length) as i32;
            let mut offset = (0x80 & branch1) << 8;
            let len: usize;
            if (branch1 & 0x40) != 0 {
                offset |= branch1 & 0x3f;
                len = 1;
            } else {
                let branch2 = memory.read_u8(self.offset + self.length + 1) as i32;
                offset |= (branch1 & 0x1f) << 8;
                offset |= branch2;
                len = 2;
            }
            let compare = (offset & 0x8000) != 0;
            offset = offset & 0x7fff;
            if offset > 0x0fff {
                offset = -(0x1fff - offset + 1);
            }
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
        let string = if let Some(ref x) = self.string {
            format!(" \"{}\"", x)
        } else {
            String::new()
        };
        let compare = if let Some(x) = self.compare {
            format!(" [{}]", x.to_string().to_uppercase())
        } else {
            String::new()
        };
        let offset = if let Some(x) = self.jump_offset {
            match x {
                0 => format!(" RFALSE"),
                1 => format!(" RTRUE"),
                _ => format!(" {:08X}", (self.offset + self.length) as i32 + x - 2),
            }
        } else {
            String::new()
        };
        write!(
            f,
            "[{:08X}] {}\t{}{}{}{}{}",
            self.offset,
            self.name().to_uppercase(),
            args.join(","),
            self.ret,
            string,
            compare,
            offset
        )
    }
}

struct Property {
    offset: usize,
    index: usize,
    length: usize,
}

impl Property {
    fn new(memory: &Memory, offset: usize) -> Property {
        let size = memory.read_u8(offset);
        Property {
            offset: offset,
            index: (size & 31) as usize,
            length: (((size & 0xe0) >> 5) + 1) as usize,
        }
    }

    fn read(&self, memory: &Memory) -> u16 {
        if self.length == 1 {
            memory.read_u8(self.offset + 1) as u16
        } else if self.length == 2 {
            memory.read_u16(self.offset + 1)
        } else {
            unimplemented!()
        }
    }

    fn write(&self, memory: &mut Memory, value: u16) {
        if self.length == 1 {
            memory.write_u8(self.offset + 1, (value & 0xff) as u8);
        } else if self.length == 2 {
            memory.write_u16(self.offset + 1, value);
        } else {
            unimplemented!()
        }
    }
}

struct Object {
    offset: usize,
    index: usize,
    attrib: usize,
    parent: usize,
    sibling: usize,
    child: usize,
    name: ZString,
}

const OBJECT_SIZE: usize = 9;
const NUM_DEFAULTS: usize = 31;
const DEFAULT_TABLE_SIZE: usize = NUM_DEFAULTS * 2;
impl Object {
    fn new(memory: &Memory, index: usize) -> Object {
        let addr = memory.read_u16(0xa) as usize + DEFAULT_TABLE_SIZE + (index - 1) * OBJECT_SIZE;
        let prop_addr = memory.read_u16(addr + 7) as usize;
        Object {
            offset: prop_addr,
            index: index,
            attrib: ((memory.read_u16(addr + 0) as usize) << 16)
                | (memory.read_u16(addr + 2) as usize),
            parent: memory.read_u8(addr + 4) as usize,
            sibling: memory.read_u8(addr + 5) as usize,
            child: memory.read_u8(addr + 6) as usize,
            name: ZString::new(memory, prop_addr + 1),
        }
    }

    fn refresh(&mut self, memory: &Memory) {
        let addr =
            memory.read_u16(0xa) as usize + DEFAULT_TABLE_SIZE + (self.index - 1) * OBJECT_SIZE;
        self.parent = memory.read_u8(addr + 4) as usize;
        self.sibling = memory.read_u8(addr + 5) as usize;
        self.child = memory.read_u8(addr + 6) as usize;
    }

    fn get_property(&self, memory: &Memory, index: usize) -> Property {
        let mut addr = self.offset + 1 + self.name.length;
        loop {
            let p = Property::new(memory, addr);
            match p {
                Property { index: 0, .. } => {
                    let default_addr = memory.read_u16(0xa) as usize + (index - 1) * 2;
                    return Property::new(memory, default_addr);
                }
                Property { index: i, .. } if i == index => return p,
                Property { length: l, .. } => addr = addr + l + 1,
            }
        }
    }

    fn get_property_opt(&self, memory: &Memory, index: usize) -> Option<Property> {
        let mut addr = self.offset + 1 + self.name.length;
        loop {
            let p = Property::new(memory, addr);
            match p {
                Property { index: 0, .. } => return None,
                Property { index: i, .. } if i == index => return Some(p),
                Property { length: l, .. } => addr = addr + l + 1,
            }
        }
    }

    fn get_next_property(&self, memory: &Memory, index: usize) -> Option<usize> {
        let mut addr = self.offset + 1 + self.name.length;
        let mut props: Vec<Property> = Vec::new();
        loop {
            let p = Property::new(memory, addr);
            match p {
                Property { index: 0, .. } => break,
                Property { length: l, .. } => addr = addr + l + 1,
            }
            props.push(p);
        }
        let mut i = props.into_iter();
        if index == 0 {
            if let Some(p) = i.next() {
                return Some(p.index);
            } else {
                // Error condition, no properties in property list (can't happen?).
                return None;
            }
        } else {
            while let Some(p) = i.next() {
                if p.index == index {
                    if let Some(p) = i.next() {
                        return Some(p.index);
                    } else {
                        return Some(0);
                    }
                }
            }
            // Error condition, requested property not found.
            return None;
        }
    }

    fn write(&self, memory: &mut Memory) {
        let addr =
            memory.read_u16(0xa) as usize + DEFAULT_TABLE_SIZE + (self.index - 1) * OBJECT_SIZE;
        memory.write_u16(addr, ((self.attrib >> 16) & 0xffff) as u16);
        memory.write_u16(addr + 2, (self.attrib & 0xffff) as u16);
        memory.write_u8(addr + 4, self.parent as u8);
        memory.write_u8(addr + 5, self.sibling as u8);
        memory.write_u8(addr + 6, self.child as u8);
        memory.write_u16(addr + 7, self.offset as u16);
    }

    fn remove(&mut self, memory: &mut Memory) {
        if self.parent != 0 {
            let mut parent = Object::new(memory, self.parent);
            let mut child = Object::new(memory, parent.child);

            if child.index == self.index {
                parent.child = self.sibling;
                parent.write(memory);
            } else {
                while child.sibling != self.index {
                    child = Object::new(memory, child.sibling);
                }
                child.sibling = self.sibling;
                child.write(memory);
            }
        }
        self.parent = 0;
        self.sibling = 0;
        self.write(memory);
    }
}

#[derive(Debug)]
struct Dictionary {
    offset: usize,
    separators: Vec<char>,
    words: Vec<ZString>,
}

impl Dictionary {
    fn new(memory: &Memory, offset: usize) -> Dictionary {
        let mut separators: Vec<char> = Vec::new();
        let mut words: Vec<ZString> = Vec::new();

        let num_separators = memory.read_u8(offset) as usize;
        for i in 0..num_separators {
            separators.push(memory.read_u8(offset + i + 1) as char);
        }

        let entry_start = offset + num_separators + 1;
        let entry_length = memory.read_u8(entry_start) as usize;
        let num_entries = memory.read_u16(entry_start + 1) as usize;

        for i in 0..num_entries {
            words.push(ZString::with_max_length(
                memory,
                entry_start + 3 + i * entry_length,
                4,
            ));
        }

        Dictionary {
            offset: offset,
            separators: separators,
            words: words,
        }
    }

    fn get_word(&self, token: &str) -> Option<ZString> {
        for word in self.words.iter() {
            // 4 byte zstring stores max 6 characters
            if word.contents.len() < 6 {
                if word.contents == token {
                    return Some(word.clone());
                }
            } else {
                if token.starts_with(&word.contents) {
                    return Some(word.clone());
                }
            }
        }
        None
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
    globals: usize,
    checksum: usize,
}

impl Header {
    fn new(mem: &Memory) -> Header {
        let dynamic_start = 0;
        let dynamic_end = mem.read_u16(0xe) as usize;
        let static_start = dynamic_end;
        let static_end = static_start + cmp::min(0xffff, mem.len());
        let high_start = mem.read_u16(0x4) as usize;
        let high_end = mem.len();
        let globals = mem.read_u16(0xc) as usize;
        let checksum = mem.read_u16(0x1c) as usize;

        Header {
            dynamic_start: dynamic_start,
            dynamic_end: dynamic_end,
            static_start: static_start,
            static_end: static_end,
            high_start: high_start,
            high_end: high_end,
            globals: globals,
            checksum: checksum,
        }
    }
}

enum MachineState {
    Continue,
    GetInput,
    Break(String),
    CleanExit,
}

pub struct Machine {
    memory: Memory,
    header: Header,
    dictionary: Dictionary,
    ip: usize,
    io: ZIO,
    finished: bool,
    rng: SmallRng,
}

impl Machine {
    fn new(memory: Memory, header: Header) -> Machine {
        Machine {
            ip: memory.read_u16(0x6) as usize,
            dictionary: Dictionary::new(&memory, memory.read_u16(0x08) as usize),
            memory: memory,
            header: header,
            io: ZIO::new(),
            finished: false,
            rng: SmallRng::from_entropy(),
        }
    }

    fn write_local(&mut self, var: u8, val: u16) {
        if let Some(frame) = self.memory.frames.last() {
            let index = frame.stack_start + (var as usize);
            self.memory.stack[index] = val;
        }
    }

    fn write_global(&mut self, var: u8, val: u16) {
        let index = var as usize * 2;
        let offset = self.header.globals + self.header.dynamic_start + index;
        self.memory.write_u16(offset, val);
    }

    fn write_var(&mut self, var: Return, val: u16) {
        match var {
            Return::Variable(x) => match x {
                x if x >= 0x10 => self.write_global(x - 0x10, val),
                x if x == 0 => self.memory.stack.push(val),
                _ => self.write_local(x - 1, val),
            },
            Return::Indirect(x) => match x {
                x if x >= 0x10 => self.write_global(x - 0x10, val),
                x if x == 0 => {
                    self.memory.stack.pop();
                    self.memory.stack.push(val)
                }
                _ => self.write_local(x - 1, val),
            },
            _ => {}
        }
    }

    fn read_local(&self, var: u8) -> u16 {
        if let Some(frame) = self.memory.frames.last() {
            let index = frame.stack_start + (var as usize);
            self.memory.stack[index]
        } else {
            0u16
        }
    }

    fn read_global(&self, var: u8) -> u16 {
        let index = var as usize * 2;
        let offset = self.header.globals + self.header.dynamic_start + index;
        self.memory.read_u16(offset)
    }

    fn read_var(&mut self, var: Operand) -> u16 {
        match var {
            Operand::Variable(x) => match x {
                x if x >= 0x10 => self.read_global(x - 0x10),
                x if x == 0 => self.memory.stack.pop().unwrap(),
                _ => self.read_local(x - 1),
            },
            Operand::Indirect(x) => match x {
                x if x >= 0x10 => self.read_global(x - 0x10),
                x if x == 0 => *self.memory.stack.last().unwrap(),
                _ => self.read_local(x - 1),
            },
            Operand::Large(x) => x,
            Operand::Small(x) => x as u16,
            Operand::Omitted => 0,
        }
    }

    fn call(&mut self, i: Instruction) {
        let addr = self.header.dynamic_start + (self.read_var(i.args[0]) as usize) * 2;
        let ret_addr = self.ip + i.length;
        let args: Vec<_> = i.args[1..].iter().map(|&a| self.read_var(a)).collect();
        if addr - self.header.dynamic_start == 0 {
            self.write_var(i.ret, 0);
            self.ip = ret_addr;
        } else {
            let num_locals = self.memory.read_u8(addr) as usize;
            self.memory.frames.push(Frame {
                addr: addr,
                stack_start: self.memory.stack.len(),
                num_locals: num_locals,
                return_storage: i.ret,
                return_addr: ret_addr,
            });
            for i in 0..num_locals {
                let arg = if i < args.len() {
                    args[i]
                } else {
                    self.memory.read_u16(addr + 1 + i * 2)
                };
                self.memory.stack.push(arg);
            }
            self.ip = addr + 1 + num_locals * 2;
        }
    }

    fn ret(&mut self, val: u16) {
        let frame = self.memory.frames.pop().unwrap();
        while self.memory.stack.len() != frame.stack_start {
            self.memory.stack.pop();
        }
        self.write_var(frame.return_storage, val);
        self.ip = frame.return_addr;
    }

    fn jump(&mut self, i: Instruction, compare: bool) {
        if let Some(x) = i.compare {
            if compare == x {
                self.ip = match i.jump_offset {
                    Some(0) => {
                        self.ret(0);
                        self.ip
                    }
                    Some(1) => {
                        self.ret(1);
                        self.ip
                    }
                    Some(x) => {
                        let offset = (i.offset + i.length) as i32 + x - 2;
                        offset as usize
                    }
                    None => self.ip,
                };
            }
        }
    }

    fn decode(&self) -> Instruction {
        Instruction::new(&self.memory, self.ip)
    }

    fn execute(&mut self, i: Instruction) -> MachineState {
        macro_rules! address {
            ($e:expr) => {
                self.header.dynamic_start + $e
            };
        }
        macro_rules! packed_address {
            ($e:expr) => {
                self.header.dynamic_start + 2 * $e
            };
        }
        macro_rules! convert_arg {
            ($e:expr, Object) => (
                {
                    let x = $e as usize;
                    if x == 0 {
                        return MachineState::Break(format!("attempted to access object 0\n"));
                    }
                    Object::new(&self.memory, x)
                }
            );
            ($e:expr, Variable) => (
                match i.args[0] {
                    Operand::Large(x) => x as u8,
                    Operand::Small(x) => x,
                    Operand::Variable(_) => self.read_var(i.args[0]) as u8,
                    _ => unimplemented!(),
                }
            );
            ($e:expr, $type:tt) => (
                $e as $type
            );
        }
        macro_rules! read_args {
            ($arg1_type:tt, $arg2_type:tt, $arg3_type:tt) => {
                (
                    convert_arg!(self.read_var(i.args[0]), $arg1_type),
                    convert_arg!(self.read_var(i.args[1]), $arg2_type),
                    convert_arg!(self.read_var(i.args[2]), $arg3_type),
                )
            };
            ($arg1_type:tt, $arg2_type:tt) => {
                (
                    convert_arg!(self.read_var(i.args[0]), $arg1_type),
                    convert_arg!(self.read_var(i.args[1]), $arg2_type),
                )
            };
            ($arg1_type:tt) => {
                convert_arg!(self.read_var(i.args[0]), $arg1_type)
            };
        }

        let oldip = self.ip;
        let length = i.length;
        match i.name() {
            "call" => {
                self.call(i);
            }
            "add" => {
                let (x, y) = read_args!(i32, i32);
                self.write_var(i.ret, ((x + y) % 0x10000) as u16);
            }
            "je" => {
                let x = read_args!(u16);
                let compare = i.args[1..].iter().any(|&b| x == self.read_var(b));
                self.jump(i, compare);
            }
            "sub" => {
                let (x, y) = read_args!(i32, i32);
                self.write_var(i.ret, ((x - y) % 0x10000) as u16);
            }
            "jz" => {
                let x = read_args!(u16);
                self.jump(i, x == 0);
            }
            "storew" => {
                let (x, y, val) = read_args!(usize, usize, u16);
                let addr = x + 2 * y;
                self.memory.write_u16(address!(addr), val);
            }
            "ret" => {
                let val = read_args!(u16);
                self.ret(val);
            }
            "loadw" => {
                let (x, y) = read_args!(usize, usize);
                let addr = x + 2 * y;
                let val = self.memory.read_u16(address!(addr));
                self.write_var(i.ret, val);
            }
            "jump" => {
                let x = read_args!(i16);
                self.ip = (self.ip as i32 + i.length as i32 + x as i32 - 2) as usize;
            }
            "put_prop" => {
                let (obj, y, val) = read_args!(Object, usize, u16);
                let prop = obj.get_property(&self.memory, address!(y));
                prop.write(&mut self.memory, val);
            }
            "store" => {
                let (x, y) = read_args!(Variable, u16);
                self.write_var(Return::Indirect(x), y);
            }
            "test_attr" => {
                let (obj, y) = read_args!(Object, usize);
                if y > 31 {
                    return MachineState::Break(format!("attribute outside allowed range\n"));
                }
                self.jump(i, (obj.attrib & (1 << (31 - y))) != 0);
            }
            "print" => {
                if let Some(s) = i.string {
                    self.io.print(&format!("{}", s));
                    if let Err(_) = self.io.flush() {}
                }
            }
            "new_line" => {
                self.io.print("\n");
            }
            "loadb" => {
                let (x, y) = read_args!(usize, usize);
                let val = self.memory.read_u8(address!(x + y)) as u16;
                self.write_var(i.ret, val);
            }
            "and" => {
                let (x, y) = read_args!(u16, u16);
                self.write_var(i.ret, x & y);
            }
            "print_num" => {
                let x = read_args!(i16);
                self.io.print(&format!("{}", x));
            }
            "inc_chk" => {
                let (x, y) = read_args!(Variable, i16);
                let old = self.read_var(Operand::Variable(x)) as i16;
                self.write_var(Return::Variable(x), (old + 1) as u16);
                self.jump(i, old + 1 > y);
            }
            "print_char" => {
                let x = read_args!(u8);
                self.io.print(&format!("{}", str::from_utf8(&[x]).unwrap()));
            }
            "rtrue" => {
                self.ret(1);
            }
            "insert_obj" => {
                let (mut obj, mut dest) = read_args!(Object, Object);

                obj.remove(&mut self.memory);

                dest.refresh(&self.memory);

                obj.sibling = dest.child;
                dest.child = obj.index;
                obj.parent = dest.index;

                obj.write(&mut self.memory);
                dest.write(&mut self.memory);
            }
            "push" => {
                let x = read_args!(u16);
                self.write_var(Return::Variable(0), x);
            }
            "pop" => {
                self.read_var(Operand::Variable(0));
            }
            "pull" => {
                let x = read_args!(Variable);
                let val = self.read_var(Operand::Variable(0));
                self.write_var(Return::Indirect(x), val);
            }
            "set_attr" => {
                let (mut obj, y) = read_args!(Object, usize);
                if y > 31 {
                    return MachineState::Break(format!("attribute outside allowed range\n"));
                }
                obj.attrib |= 1 << (31 - y);
                obj.write(&mut self.memory);
            }
            "jin" => {
                let (obj, y) = read_args!(Object, usize);
                self.jump(i, obj.parent == y);
            }
            "print_obj" => {
                let obj = read_args!(Object);
                self.io.print(&format!("{}", obj.name));
            }
            "get_parent" => {
                let obj = read_args!(Object);
                self.write_var(i.ret, obj.parent as u16);
            }
            "get_prop" => {
                let (obj, y) = read_args!(Object, usize);
                let prop = obj.get_property(&self.memory, y);
                let val = prop.read(&self.memory);
                self.write_var(i.ret, val);
            }
            "jg" => {
                let (x, y) = read_args!(i16, i16);
                self.jump(i, x > y);
            }
            "get_child" => {
                let obj = read_args!(Object);
                self.write_var(i.ret, obj.child as u16);
                self.jump(i, obj.child != 0);
            }
            "get_sibling" => {
                let obj = read_args!(Object);
                self.write_var(i.ret, obj.sibling as u16);
                self.jump(i, obj.sibling != 0);
            }
            "rfalse" => {
                self.ret(0);
            }
            "inc" => {
                let x = read_args!(Variable);
                let old = self.read_var(Operand::Variable(x)) as i32;
                self.write_var(Return::Variable(x), ((old + 1) % 0x10000) as u16);
            }
            "jl" => {
                let (x, y) = read_args!(i16, i16);
                self.jump(i, x < y);
            }
            "ret_popped" => {
                let x = self.read_var(Operand::Variable(0));
                self.ret(x);
            }
            "sread" => {
                if !self.io.poll_input() {
                    return MachineState::GetInput;
                }
                let x = address!(self.read_var(i.args[0]) as usize);
                let y = address!(self.read_var(i.args[1]) as usize);

                let mut input = self.io.input();
                input = input.trim().to_lowercase();
                let max_length = std::cmp::min(self.memory.read_u8(x) as usize, input.len());

                for (i, c) in input[..max_length].bytes().enumerate() {
                    self.memory.write_u8(x + 1 + i, c);
                }
                self.memory.write_u8(x + max_length + 1, 0);

                let tokens: Vec<_> = input
                    .split(|c| c == ' ' || self.dictionary.separators.iter().any(|x| *x == c))
                    .collect();
                let max_parse = std::cmp::min(self.memory.read_u8(y) as usize, tokens.len());
                self.memory.write_u8(y + 1, max_parse as u8);
                for (i, token) in tokens[..max_parse].iter().enumerate() {
                    let offset = y + 2 + 4 * i;
                    if let Some(zs) = self.dictionary.get_word(&token) {
                        self.memory.write_u16(offset, zs.offset as u16);
                    } else {
                        self.memory.write_u16(offset, 0);
                    }
                    self.memory.write_u8(offset + 2, token.len() as u8);
                    let index = input.find(token).unwrap();
                    self.memory.write_u8(offset + 3, index as u8 + 1);
                }
            }
            "dec_chk" => {
                let (x, y) = read_args!(Variable, i16);
                let old = self.read_var(Operand::Variable(x)) as i16;
                self.write_var(Return::Variable(x), (old - 1) as u16);
                self.jump(i, old - 1 < y);
            }
            "mul" => {
                let (x, y) = read_args!(i64, i64);
                self.write_var(i.ret, ((x * y) % 0x10000) as u16);
            }
            "test" => {
                let (x, y) = read_args!(u16, u16);
                self.jump(i, (x & y) == y);
            }
            "storeb" => {
                let (x, y, val) = read_args!(usize, usize, u8);
                self.memory.write_u8(address!(x + y), val);
            }
            "clear_attr" => {
                let (mut obj, y) = read_args!(Object, usize);
                if y > 31 {
                    return MachineState::Break(format!("attribute outside allowed range\n"));
                }
                obj.attrib &= !(1 << (31 - y));
                obj.write(&mut self.memory);
            }
            "get_prop_addr" => {
                let (obj, y) = read_args!(Object, usize);
                if let Some(prop) = obj.get_property_opt(&self.memory, y) {
                    self.write_var(i.ret, prop.offset as u16 + 1);
                } else {
                    self.write_var(i.ret, 0);
                }
            }
            "get_prop_len" => {
                let x = read_args!(usize);
                if x == 0 {
                    self.write_var(i.ret, 0);
                } else {
                    let property = Property::new(&self.memory, x - 1);
                    self.write_var(i.ret, property.length as u16);
                }
            }
            "print_paddr" => {
                let x = read_args!(usize);
                let zs = ZString::new(&self.memory, packed_address!(x));
                self.io.print(&format!("{}", zs));
            }
            "dec" => {
                let x = read_args!(Variable);
                let old = self.read_var(Operand::Variable(x)) as i32;
                self.write_var(Return::Variable(x), ((old - 1) % 0x10000) as u16);
            }
            "print_ret" => {
                if let Some(s) = i.string {
                    self.io.print(&format!("{}\n", s));
                }
                self.ret(1);
            }
            "div" => {
                let (x, y) = read_args!(i16, i16);
                if y == 0 {
                    return MachineState::Break(format!("divide by zero\n"));
                }
                self.write_var(i.ret, (x / y) as u16);
            }
            "print_addr" => {
                let x = read_args!(usize);
                let zs = ZString::new(&self.memory, address!(x));
                self.io.print(&format!("{}", zs));
            }
            "not" => {
                let x = read_args!(u16);
                self.write_var(i.ret, !x);
            }
            "or" => {
                let (x, y) = read_args!(u16, u16);
                self.write_var(i.ret, x | y);
            }
            "mod" => {
                let (x, y) = read_args!(i16, i16);
                if y == 0 {
                    return MachineState::Break(format!("divide by zero\n"));
                }
                self.write_var(i.ret, (x % y) as u16);
            }
            "remove_obj" => {
                let mut obj = read_args!(Object);
                obj.remove(&mut self.memory);
            }
            "random" => {
                let range = read_args!(i16);
                if range <= 0 {
                    self.rng = SmallRng::seed_from_u64(range as u64);
                    self.write_var(i.ret, 0);
                } else {
                    let x = self.rng.next_u32() as u16;
                    let val = x % range as u16 + 1;
                    self.write_var(i.ret, val);
                }
            }
            "get_next_prop" => {
                let (obj, y) = read_args!(Object, usize);
                if let Some(index) = obj.get_next_property(&self.memory, y) {
                    self.write_var(i.ret, index as u16);
                } else {
                    return MachineState::Break(format!("could not find property\n"));
                }
            }
            "load" => {
                let x = read_args!(Variable);
                let val = self.read_var(Operand::Indirect(x));
                self.write_var(i.ret, val);
            }
            "verify" => {
                self.jump(i, true);
            }
            "quit" => {
                return MachineState::CleanExit;
            }
            _ => return MachineState::Break(format!("unimplemented instruction:\n{}", i)),
        }
        if self.ip == oldip {
            self.ip += length;
        }
        MachineState::Continue
    }

    fn step(&mut self) {
        if !self.finished {
            loop {
                let i = self.decode();
                #[cfg(debug_assertions)]
                self.io.log(&format!("{}", i));
                match self.execute(i) {
                    MachineState::Continue => {}
                    MachineState::Break(s) => {
                        self.io.log(&s);
                        self.finished = true;
                        break;
                    }
                    MachineState::GetInput => {
                        break;
                    }
                    MachineState::CleanExit => {
                        self.finished = true;
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(feature = "cli")]
fn open_z3(filename: &str) -> Result<Machine, std::io::Error> {
    let buffer = std::fs::read(filename)?;

    let memory = Memory::new(&buffer);
    let header = Header::new(&memory);

    Ok(Machine::new(memory, header))
}

#[cfg(feature = "cli")]
fn get_machine() -> Machine {
    use clap::{App, Arg};
    let matches = App::new("rustzork")
        .version("1.0")
        .about("Interpreter for V3 zmachine spec.")
        .arg(
            Arg::with_name("file")
                .help("Path to the .z3 file to run")
                .index(1)
                .required(false),
        )
        .get_matches();

    let filename = matches.value_of("file").unwrap_or("zork.z3");

    let machine = match open_z3(filename) {
        Ok(x) => x,
        Err(e) => {
            println!("Error opening file: {}", e);
            std::process::exit(1);
        }
    };
    machine
}

#[cfg(not(feature = "cli"))]
fn get_machine() -> Machine {
    let bytes = include_bytes!("../zork.z3");
    let memory = Memory::new(bytes);
    let header = Header::new(&memory);

    Machine::new(memory, header)
}

#[cfg(not(feature = "cli"))]
#[no_mangle]
pub extern "C" fn initialize() -> *mut Machine {
    let machine = Box::new(get_machine());
    Box::into_raw(machine)
}

#[cfg(not(feature = "cli"))]
#[no_mangle]
pub extern "C" fn key_pressed(machine: *mut Machine, key: u8) {
    let mut machine: Box<Machine> = unsafe { Box::from_raw(machine) };
    machine.io.key_down(key);
    machine.io.draw();
    std::mem::forget(machine);
}

#[cfg(not(feature = "cli"))]
#[no_mangle]
pub extern "C" fn update(machine: *mut Machine) {
    let mut machine: Box<Machine> = unsafe { Box::from_raw(machine) };
    machine.step();
    machine.io.draw();
    std::mem::forget(machine);
}

fn main() {
    let mut machine = get_machine();

    machine.step();
}
