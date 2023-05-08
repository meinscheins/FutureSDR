#![allow(clippy::new_ret_no_self)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::excessive_precision)]

use futuresdr::num_complex::Complex32;

mod bch;
pub use bch::BCH;

mod crc32;
pub use crc32::CRC32;

mod crc16;
pub use crc16::CRC16;

mod encoder;
pub use encoder::Encoder;

mod mls;
pub use mls::MLS;

mod polar;
pub use polar::PolarEncoder;


pub fn base37_map(c: char) -> u8 {
    let c = c as u8;
    //test if char is alphanumerical anc convert it to base37
    if c >= 48 && c <= 57 {
        return c - 48 + 1;
    }
    if c >= 97 && c <= 122 {
        return c - 97 + 11;
    }
    if c >= 65 && c <= 90 {
        return c - 65 + 11;
    }
	return 0
}

pub fn base37(str: String) -> u64 {
    let mut acc: u64 = 0;
    for c in str.chars() {
        acc = 37 * acc * (base37_map(c) as u64);
    }
    return acc
}

pub fn nrz(bit: u8) -> isize {
    if bit == 0 {
        return 1;
    } else {
        return -1; 
    }
}

pub fn bin(carrier: isize, carrier_offset: isize, symbol_length: isize) -> isize {
    return (carrier + carrier_offset + symbol_length) % symbol_length;
}

pub fn xor_be_bit(buf: &mut Vec<u8>, pos: usize, value: u8) {
    let mut val = 0;
    if value != 0 {
        val = 1;
    }
    buf[pos/8] ^= val<<(7-pos%8);
} 

pub fn xor_le_bit(buf: &mut Vec<u8>, pos: usize, value: u8) {
    let mut val = 0;
    if value != 0 {
        val = 1;
    }
    buf[pos/8] ^= val<<(pos%8)
}

pub fn set_be_bit(buf: &mut Vec<u8>, pos: usize, value: u8) {
    let mut val = 0;
    if value != 0 {
        val = 1;
    }
    buf[pos/8] = (!(1<<(7-pos%8))&buf[pos/8])|(val<<(7-pos%8));
}

pub fn set_le_bit(buf: &mut Vec<u8>, pos: usize, value: u8) {
    let mut val = 0;
    if value != 0 {
        val = 1;
    }
    buf[pos/8] = (!(1<<(pos%8))&buf[pos/8])|(val<<(pos%8));
}

pub fn get_be_bit(buf: &mut Vec<u8>, pos: usize) -> u8 {
    return (buf[pos/8]>>(7-pos%8))&1;
}

pub fn get_le_bit(buf: &mut Vec<u8>, pos: usize) -> u8 {
    return (buf[pos/8]>>(pos%8))&1;
}


#[derive(Clone, Copy, Debug)]
pub enum Modulation {
    Bpsk,
    Qpsk,
}

impl Modulation {
    /// bits per symbol
    pub fn n_bpsc(&self) -> usize {
        match self {
            Modulation::Bpsk => 1,
            Modulation::Qpsk => 2,
        }
    }
    pub fn map(&self, code: &Vec<i8>, index: usize) -> Complex32 {
        match self {
            Modulation::Bpsk => {
                Complex32::new(code[index] as f32, 0.0)
            },
            Modulation::Qpsk => {
                Complex32::new(code[index] as f32, code[index+1] as f32)
            },
        }
    }

    pub fn demap(&self, i: &Complex32) -> Vec<i8> {
        match self {
            Modulation::Bpsk => {
                vec![i.re as i8]
            },
            Modulation::Qpsk => {
                vec![i.re as i8, i.im as i8]
            }
        }
    }
}




