extern crate clap;

use std::cmp;
use std::fs::File;
use std::io::Read;
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

#[derive(Copy, Clone)]
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

struct Machine {
    memory: Memory,
    header: Header,
    ip: usize,
}

impl Machine {
    fn new(memory: Memory, header: Header) -> Machine {
        Machine {
            ip: memory.read_u16(0x6) as usize,
            memory: memory,
            header: header,
        }
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
    let machine = match open_z3(filename) {
        Ok(x) => x,
        Err(e) => {
            println!("Error opening file: {}", e);
            std::process::exit(1);
        }
    };
}
