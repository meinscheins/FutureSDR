use std::{vec::Vec, isize};
use crate::{CRC32, nrz, get_le_bit};

fn get(bits: Vec<u32>, i: usize) -> bool{
   let tmp = (bits[i/32] >> (i%32)) & 1;
   if  tmp == 0 {
    return false
   }
   return true
}

pub fn polar_sys_enc(codeword: &mut Vec<u8>, message: &mut Vec<u8>, frozen: Vec<u32>, level: usize) {
    let length: usize = 1 << level;
    let mut i = 0;
    let mut j = 0;
    let mut h = 0;
    while (i < length) {
        let mut msg0: u8 = 1;
        let mut msg1: u8 = 1;
        if !get(frozen, i) {
            msg0 = message[0];
            message[0] += 1;
        } 
        
        if !get(frozen, i+1) {
            msg1 = message[0];
            message[0] += 1;
        } 
        codeword[i] = msg0 * msg1;
        codeword[i+1] = msg1;
        i += 2;
    } 
    i = 0;
    j = 0;
    h = 2;
    while(h < length) {
        while(i < length) {
                while(j < i + h) {
                    let tmp: u8 = codeword[j+h];
                    codeword[j] *= tmp;
                    j += 1;
                }
            i += 2*h;
        }
        h *= 2;
    }
    i = 0;
    while(i < length) {
        let mut msg0: u8 = 1;
        let mut msg1: u8 = 1;
        if !get(frozen, i) {
            msg0 = codeword[i];
        } 
        if !get(frozen, i+1) {
            msg1 = codeword[i+1];
        } 
        codeword[i] = msg0 * msg1;
        codeword[i+1] = msg1;
        i += 2;
    }
    i = 0;
    j = 0;
    h = 2;
    while(h < length) {
        while(i < length) {
                while(j < i + h) {
                    let tmp: u8 = codeword[j+h];
                    codeword[j] *= tmp;
                    j += 1;
                }
            i += 2*h;
        }
        h *= 2;
    }
}

pub struct PolarEncoder { 
    crc: CRC32,
    code_order: usize,
    max_bits: usize,
    mesg: Vec<i8>
}

impl PolarEncoder {
    pub fn new(crc_poly: u32) -> PolarEncoder {
        let max_bits = 1360 + 32;
        let mut mesg = vec![0; max_bits];
        PolarEncoder { crc: CRC32::new(crc_poly, 0), 
            code_order: 11, 
            max_bits: max_bits, 
            mesg: mesg 
        }
    }

    pub fn encode(&mut self, code: &mut Vec<u8>, message: &mut Vec<u8>, frozen_bits: Vec<u32>, data_bits: usize){
        for i in 0..data_bits {
            self.mesg[i] = nrz(get_le_bit(message, i)) as i8;
        }
        self.crc.reset();
        for i in 0..data_bits/8 {
            self.crc.crc_u8(message[i]);
        }
        for i in 0..32 {
            self.mesg[i + data_bits] = nrz(((self.crc.crc() >> i) & 1) as u8 ) as i8;
        }
        polar_sys_enc(code, &mut self.mesg, frozen_bits, self.code_order);
    }
}