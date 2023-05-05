use futuresdr::num_complex::Complex32;
use rustfft::num_complex::Complex;
use rustfft::num_traits::clamp;
use rustfft::{self,Fft,FftPlanner};
use std::cmp::max;
use std::sync::Arc;
use std::vec::Vec;
use crate::{Modulation, MLS, bin, nrz, set_be_bit, CRC16, BCH, get_be_bit};

pub struct Encoder {
    rate: isize,
    code_order: isize,
	mod_bits: isize,
	code_len: isize,
	symbol_count: isize,
	symbol_length: isize,
	guard_length: isize,
	extended_length: isize,
	max_bits: isize,
	cor_seq_len: isize,
	cor_seq_off: isize,
	cor_seq_poly: isize,
	pre_seq_len: isize,
	pre_seq_off: isize,
	pre_seq_poly: isize,
	pay_car_cnt: isize,
	pay_car_off: isize,
	fancy_off: isize,
	noise_poly: isize,
    ifft: Arc<dyn Fft<f32>>,
    fft: Arc<dyn Fft<f32>>,
    ifft_papr: Arc<dyn Fft<f32>>,
    fft_papr: Arc<dyn Fft<f32>>,
    crc: CRC16,
	bch: BCH,
	noise_seq: MLS,
	//PolarEncoder<code_type> polar;
    freq: Vec<Complex32>,
    guard: Vec<Complex32>,
    temp: Vec<Complex32>,
    prev: Vec<Complex32>,
    mesg: Vec<u8>,
    call: Vec<u8>,
    code: Vec<i8>,
    meta_data: u64, 
    operation_mode: isize,
	carrier_offset: isize,
	symbol_number: isize,
	count_down: isize,
	fancy_line: isize,
	noise_count: isize,
}

impl Encoder{

    pub fn new(&mut self, rate: isize, noise_poly: isize, crc_poly: u16, bch_minimal_polynomials: Vec<usize>) -> Encoder {
        let mut planner = FftPlanner::<f32>::new();
        let mut factor: isize = 4;
        if rate <= 16000 {
            factor = 1; 
        }
        Encoder { rate: rate, 
            code_order: 11, 
            mod_bits: 2, 
            code_len: 1 << self.code_order, 
            symbol_count: 4, 
            symbol_length: (1280 * rate) / 8000, 
            guard_length: self.symbol_length / 8, 
            extended_length: self.symbol_length + self.guard_length, 
            max_bits: 1360, 
            cor_seq_len: 127, 
            cor_seq_off: 1 - self.cor_seq_len, 
            cor_seq_poly: 0b10001001, 
            pre_seq_len: 255, 
            pre_seq_off: - self.pre_seq_len / 2, 
            pre_seq_poly: 0b100101011, 
            pay_car_cnt: 256, 
            pay_car_off: - self.pay_car_cnt / 2, 
            fancy_off: - (8 * 9 * 3) / 2, 
            noise_poly: 0b100101010001, 
            ifft: planner.plan_fft_inverse(self.symbol_length as usize), 
            fft: planner.plan_fft_forward(self.symbol_length as usize), 
            ifft_papr: planner.plan_fft_inverse(self.symbol_length as usize * factor as usize), 
            fft_papr: planner.plan_fft_forward(self.symbol_length as usize * factor as usize),
            crc: CRC16::new(crc_poly, 0),
	        bch: BCH::new(255, 71, bch_minimal_polynomials),
            noise_seq: MLS::new(0b100000000000000001001, 1), 
            //PolarEncoder<code_type> polar;
            freq: vec![Complex32::new(0.0, 0.0); self.symbol_length as usize], 
            guard: vec![Complex32::new(0.0, 0.0); self.guard_length as usize], 
            temp: vec![Complex32::new(0.0, 0.0); self.extended_length as usize], 
            prev: vec![Complex32::new(0.0, 0.0); self.pay_car_cnt as usize], 
            mesg: vec![0; (self.max_bits/8) as usize], 
            call: vec![0; 9], 
            code: vec![0; self.code_len as usize], 
            meta_data: 0, 
            operation_mode: 0, 
            carrier_offset: 0, 
            symbol_number: self.symbol_count, 
            count_down: 0, 
            fancy_line: 0, 
            noise_count: 0 
        }
    }
    pub fn transform(&mut self, input: &mut Vec<Complex32>, output: &mut Vec<Complex32>, papr_reduction: bool) {
        if papr_reduction && self.rate <= 16000 {
            self.improve_papr(input, 4)
        }
        self.ifft.process(input);
        for i in 0..input.len() {
            output[i] = input[i] / ((self.symbol_length * 8) as f32).sqrt();
        }

    }
    
    pub fn improve_papr(&mut self, freq: &mut Vec<Complex32>, fact: usize) {
        let size = freq.len();
        let mut over: Vec<Complex32> = vec!(Complex32::new(0.0, 0.0); size*fact);
        let mut used: Vec<bool> = vec!(false; size) ;
    
        for i in 0..size {
            used[i] = (freq[i].re != 0.0) || (freq[i].im != 0.0);
        }
        for i in 0..size/2 {
            over[i] = freq[i];
        }
        for i in (size/2)..size {
            over[freq.len() * (fact - 1) + i] = freq[i];
        }
        self.ifft_papr.process(&mut over);
        let factor = Complex32::new(1.0 / ((fact*size) as f32).sqrt(), 0.0);
        for i in 0..over.len() {
            over[i] *= factor;
        }
        for i in 0..over.len() {
            let amp = (over[i].re).abs().max((over[i].im).abs());
            if amp > 1.0 {
                over[i] /= amp;
            }
        }
        self.fft_papr.process(&mut over);
        for i in 0..size/2 {
            if used[i] {
                freq[i] = factor * over[i];
            }
        }
        for i in size/2..size {
            if used[i] {
                freq[i] = factor * over[size * (fact - 1) + i];
            }
        }
    
    }

    pub fn next_sample(&mut self, samples: &mut Vec<i16>, signal: Complex32, channel: isize, i: usize) {
        match channel {
            1 => {
                samples[2 * i] = clamp((32767.0 * signal.re).round() as i16, -32768, 32767);
				samples[2 * i + 1] = 0; 
            }
            2 => {
                samples[2 * i] = 0;
				samples[2 * i + 1] = clamp((32767.0 * signal.re).round() as i16, -32768, 32767); 
            }
            4 => {
                samples[2 * i] = clamp((32767.0 * signal.re).round() as i16, -32768, 32767);
				samples[2 * i + 1] = clamp((32767.0 * signal.im).round() as i16, -32768, 32767); 
            }
            _ => samples[i] = clamp((32767.0 * signal.re).round() as i16, -32768, 32767),
        }
    }
    
    pub fn schmidl_cox(&mut self) -> Vec<Complex32> {
        let mut seq: MLS = MLS::new(self.cor_seq_poly, 1);
        let factor: f32 = (2.0 * self.symbol_length as f32 / self.cor_seq_len as f32).sqrt();
        let mut freq: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); self.symbol_length as usize];
        freq[bin(self.cor_seq_off - 2, self.carrier_offset, self.symbol_length) as usize] = Complex32::new(factor, 0.0);
        for i in 0..self.cor_seq_len {
            freq[bin(2 * i + self.cor_seq_off, self.carrier_offset, self.symbol_length)as usize] = Complex32::new(nrz(seq.mls() as u8) as f32, 0.0);
        }
        for i in 0..self.cor_seq_len {
            let temp = freq[bin(2 * (i - 1) + self.cor_seq_off, self.carrier_offset, self.symbol_length)as usize];
            freq[bin(2 * i + self.cor_seq_off, self.carrier_offset, self.symbol_length)as usize] *= temp;
        }
        let mut out: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); self.extended_length as usize];
        self.transform(&mut freq, &mut out, false);
        return out;
    }

    pub fn preamble(&mut self) -> Vec<Complex32>  { 
        let mut data: Vec<u8> = vec![0; 9];
        let mut parity: Vec<u8> = vec![0; 23];
        let mut freq: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); self.symbol_length as usize];
        let mut out: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); self.extended_length as usize];
        for i in 0..55 {
            set_be_bit(&mut data, i, (self.meta_data >> i) as u8 & 1);
        }
        self.crc.reset();
        let mut cs: u16 = self.crc.crc_u64(self.meta_data << 9);
        for i in 0..16 {
            set_be_bit(&mut data, i + 55, (cs >> i) as u8 & 1);
        }
        self.bch.bch(&mut data, &mut parity);
        let mut seq: MLS = MLS::new(self.pre_seq_poly, 1);
        let factor: f32 = (self.symbol_length as f32 / self.pre_seq_len as f32).sqrt();
        freq[bin(self.pre_seq_off - 1, self.carrier_offset, self.symbol_length) as usize] 
            = Complex32::new(factor, 0.0);
        for i in 0..71 {
            freq[bin(i + self.pre_seq_off, self.carrier_offset, self.symbol_length) as usize]
                = Complex32::new(nrz(get_be_bit(& mut data, i as usize)) as f32, 0.0);
        }
        for i in 71..self.pre_seq_len {
            freq[bin(i + self.pre_seq_off, self.carrier_offset, self.symbol_length) as usize]
                = Complex32::new(nrz(get_be_bit(& mut parity, (i - 71) as usize)) as f32, 0.0);
        }
        for i in 0..self.pre_seq_len {
            let tmp = freq[bin(i -1 + self.pre_seq_off, self.carrier_offset, self.symbol_length) as usize];
            freq[bin(i + self.pre_seq_off, self.carrier_offset, self.symbol_length) as usize]
                *= tmp;
        }
        for i in 0..self.pre_seq_len {
            freq[bin(i + self.pre_seq_off, self.carrier_offset, self.symbol_length) as usize]
                *= nrz(seq.mls() as u8) as f32;
        }
        for i in 0..self.pay_car_cnt {
            self.prev[i as usize] = freq[bin(i + self.pay_car_off, self.carrier_offset, self.symbol_length) as usize];
        }
        self.transform(&mut freq, &mut out, true);
        return out;
    }

    pub fn noise_symbol(&mut self) -> Vec<Complex32> {
        let factor: f32 = (self.symbol_length as f32 / self.pay_car_cnt as f32).sqrt();
        let mut freq: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); self.symbol_length as usize];
        let mut out: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); self.extended_length as usize];
        for i in 0..self.pay_car_cnt {
            freq[bin(i + self.pay_car_off, self.carrier_offset, self.symbol_length) as usize]
                *= factor * Complex32::new(nrz(self.noise_seq.mls() as u8) as f32, nrz(self.noise_seq.mls() as u8) as f32);
        }
        self.transform(&mut freq, &mut out, false);
        return out;
    }

    pub fn payload_symbol(&mut self) {
        let mut freq: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); self.symbol_length as usize];
        let mut out: Vec<Complex32> = vec![Complex32::new(0.0, 0.0); self.extended_length as usize];
        for i in 0..self.pay_car_cnt {
            freq[bin(i + self.pay_car_off, self.carrier_offset, self.symbol_length) as usize]
                *= 
        }
    }

    pub fn silence(&mut self) -> Vec<Complex32> {
        return vec![Complex32::new(0.0, 0.0); self.extended_length as usize];
    }

}