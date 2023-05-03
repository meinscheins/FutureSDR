#![allow(clippy::new_ret_no_self)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::excessive_precision)]

use futuresdr::num_complex::Complex32;
use futuresdr::runtime::StreamInput;
use futuresdr::runtime::StreamOutput;

mod bch;
pub use bch::BCH;

mod crc16;
pub use crc16::CRC16;

mod encoder;
pub use encoder::Encoder;

mod mls;
pub use mls::MLS;


pub fn base37_map(c: char) -> u8 {
    let c = c as u8;
    //test if char is alphanumerical anc convert it to base37
    if (c >= 48 && c <= 57) {
        return c - 48 + 1;
    }
    if (c >= 97 && c <= 122) {
        return c - 97 + 11;
    }
    if (c >= 65 && c <= 90) {
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

pub fn nrz(bit: bool) -> isize {
    match bit {
        true => -1,
        false => 1 ,
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
    Qam16,
    Qam64,
}

impl Modulation {
    /// bits per symbol
    pub fn n_bpsc(&self) -> usize {
        match self {
            Modulation::Bpsk => 1,
            Modulation::Qpsk => 2,
            Modulation::Qam16 => 4,
            Modulation::Qam64 => 6,
        }
    }
    pub fn map(&self, i: u8) -> Complex32 {
        match self {
            Modulation::Bpsk => {
                const BPSK: [Complex32; 2] = [Complex32::new(-1.0, 0.0), Complex32::new(1.0, 0.0)];
                BPSK[i as usize]
            }
            Modulation::Qpsk => {
                const LEVEL: f32 = std::f32::consts::FRAC_1_SQRT_2;
                const QPSK: [Complex32; 4] = [
                    Complex32::new(-LEVEL, -LEVEL),
                    Complex32::new(LEVEL, -LEVEL),
                    Complex32::new(-LEVEL, LEVEL),
                    Complex32::new(LEVEL, LEVEL),
                ];
                QPSK[i as usize]
            }
            Modulation::Qam16 => {
                const LEVEL: f32 = 0.31622776601683794;
                const QAM16: [Complex32; 16] = [
                    Complex32::new(-3.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, 1.0 * LEVEL),
                ];
                QAM16[i as usize]
            }
            Modulation::Qam64 => {
                const LEVEL: f32 = 0.1543033499620919;
                const QAM64: [Complex32; 64] = [
                    Complex32::new(-7.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, -7.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, 7.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, -1.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, 1.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, -5.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, 5.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, -3.0 * LEVEL),
                    Complex32::new(-7.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(7.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(-1.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(1.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(-5.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(5.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(-3.0 * LEVEL, 3.0 * LEVEL),
                    Complex32::new(3.0 * LEVEL, 3.0 * LEVEL),
                ];
                QAM64[i as usize]
            }
        }
    }

    pub fn demap(&self, i: &Complex32) -> u8 {
        match self {
            Modulation::Bpsk => (i.re > 0.0) as u8,
            Modulation::Qpsk => 2 * (i.im > 0.0) as u8 + (i.re > 0.0) as u8,
            Modulation::Qam16 => {
                let mut ret = 0u8;
                const LEVEL: f32 = 0.6324555320336759;
                let re = i.re;
                let im = i.im;

                ret |= u8::from(re > 0.0);
                ret |= if re.abs() < LEVEL { 2 } else { 0 };
                ret |= if im > 0.0 { 4 } else { 0 };
                ret |= if im.abs() < LEVEL { 8 } else { 0 };
                ret
            }
            Modulation::Qam64 => {
                const LEVEL: f32 = 0.1543033499620919;

                let mut ret = 0;
                let re = i.re;
                let im = i.im;

                ret |= u8::from(re > 0.0);
                ret |= if re.abs() < (4.0 * LEVEL) { 2 } else { 0 };
                ret |= if (re.abs() < (6.0 * LEVEL)) && (re.abs() > (2.0 * LEVEL)) {
                    4
                } else {
                    0
                };
                ret |= if im > 0.0 { 8 } else { 0 };
                ret |= if im.abs() < (4.0 * LEVEL) { 16 } else { 0 };
                ret |= if (im.abs() < (6.0 * LEVEL)) && (im.abs() > (2.0 * LEVEL)) {
                    32
                } else {
                    0
                };

                ret
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(non_camel_case_types)]
pub enum Mcs {
    Bpsk_1_2,
    Bpsk_3_4,
    Qpsk_1_2,
    Qpsk_3_4,
    Qam16_1_2,
    Qam16_3_4,
    Qam64_2_3,
    Qam64_3_4,
}

impl Mcs {
    pub fn depuncture_pattern(&self) -> &'static [usize] {
        match self {
            Mcs::Bpsk_1_2 | Mcs::Qpsk_1_2 | Mcs::Qam16_1_2 => &[1, 1],
            Mcs::Bpsk_3_4 | Mcs::Qpsk_3_4 | Mcs::Qam16_3_4 | Mcs::Qam64_3_4 => &[1, 1, 1, 0, 0, 1],
            Mcs::Qam64_2_3 => &[1, 1, 1, 0],
        }
    }

    pub fn modulation(&self) -> Modulation {
        match self {
            Mcs::Bpsk_1_2 => Modulation::Bpsk,
            Mcs::Bpsk_3_4 => Modulation::Bpsk,
            Mcs::Qpsk_1_2 => Modulation::Qpsk,
            Mcs::Qpsk_3_4 => Modulation::Qpsk,
            Mcs::Qam16_1_2 => Modulation::Qam16,
            Mcs::Qam16_3_4 => Modulation::Qam16,
            Mcs::Qam64_2_3 => Modulation::Qam64,
            Mcs::Qam64_3_4 => Modulation::Qam64,
        }
    }

    // coded bits per symbol
    pub fn n_cbps(&self) -> usize {
        match self {
            Mcs::Bpsk_1_2 => 48,
            Mcs::Bpsk_3_4 => 48,
            Mcs::Qpsk_1_2 => 96,
            Mcs::Qpsk_3_4 => 96,
            Mcs::Qam16_1_2 => 192,
            Mcs::Qam16_3_4 => 192,
            Mcs::Qam64_2_3 => 288,
            Mcs::Qam64_3_4 => 288,
        }
    }

    // data bits per symbol
    pub fn n_dbps(&self) -> usize {
        match self {
            Mcs::Bpsk_1_2 => 24,
            Mcs::Bpsk_3_4 => 36,
            Mcs::Qpsk_1_2 => 48,
            Mcs::Qpsk_3_4 => 72,
            Mcs::Qam16_1_2 => 96,
            Mcs::Qam16_3_4 => 144,
            Mcs::Qam64_2_3 => 192,
            Mcs::Qam64_3_4 => 216,
        }
    }
    // rate field for signal field
    pub fn rate_field(&self) -> u8 {
        match self {
            Mcs::Bpsk_1_2 => 0x0d,
            Mcs::Bpsk_3_4 => 0x0f,
            Mcs::Qpsk_1_2 => 0x05,
            Mcs::Qpsk_3_4 => 0x07,
            Mcs::Qam16_1_2 => 0x09,
            Mcs::Qam16_3_4 => 0x0b,
            Mcs::Qam64_2_3 => 0x01,
            Mcs::Qam64_3_4 => 0x03,
        }
    }

    pub fn parse(s: &str) -> Result<Mcs, String> {
        let mut m = s.to_string().replace(['-', '_'], "");
        m.make_ascii_lowercase();
        match m.as_str() {
            "bpsk12" => Ok(Mcs::Bpsk_1_2),
            "bpsk34" => Ok(Mcs::Bpsk_3_4),
            "qpsk12" => Ok(Mcs::Qpsk_1_2),
            "qpsk34" => Ok(Mcs::Qpsk_3_4),
            "qam1612" => Ok(Mcs::Qam16_1_2),
            "qam1634" => Ok(Mcs::Qam16_3_4),
            "qam6423" => Ok(Mcs::Qam64_2_3),
            "qam6434" => Ok(Mcs::Qam64_3_4),
            _ => Err(format!("Invalid MCS {s}")),
        }
    }
}



