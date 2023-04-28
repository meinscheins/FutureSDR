use futuresdr::num_complex::Complex32;
use rustfft::{self,Fft,FftPlanner};
use std::sync::Arc;
use std::vec::Vec;

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
    ifft: Arc<dyn Fft<Complex32>>,
    fft: Arc<dyn Fft<Complex32>>,
    ifft_papr: Arc<dyn Fft<Complex32>>,
    fft_papr: Arc<dyn Fft<Complex32>>,


    //CODE::CRC<uint16_t> crc;
	//CODE::BoseChaudhuriHocquenghemEncoder<255, 71> bch;
	//CODE::MLS noise_seq;
	//PolarEncoder<code_type> polar;

    freq: Vec<Complex32>,
    guard: Vec<Complex32>,
    temp: Vec<Complex32>,
    prev: [Complex32; 256],
    mesg: [u8; 1360/8],
    call: [u8; 9],
    //code_len = 1 << code_order = 1 << 11
    code: [i8; 1 << 11],
    meta_data: u64, 
    operation_mode: isize,
	carrier_offset: isize,
	symbol_number: isize,
	count_down: isize,
	fancy_line: isize,
	noise_count: isize,
}

impl Encoder{

    //pub fn new(&mut self, rate:isize)
    pub fn transform<const FREQ_LEN: usize, const RET_LEN: usize>(&mut self, freq: [Complex32; FREQ_LEN], rate: usize, papr_reduction: bool) -> [Complex32; RET_LEN] {
        if papr_reduction && (rate <= 16000) {
            
        }
    
    }
    
    pub fn improve_papr(&mut self, freq: &Vec<Complex32>, fact: usize) {
        let mut over: Vec<Complex32> = vec!(Complex32::new(0.0, 0.0); freq.len()*fact);
        let mut used: Vec<bool> = vec!(false; freq.len()) ;
    
        for i in 0..freq.len() {
            used[i] = (freq[i].re != 0.0) || (freq[i].im != 0.0);
        }
        for i in 0..(freq.len()/2) {
            over[i] = freq[i];
        }
        for i in (freq.len()/2)..freq.len() {
            over[freq.len() * (fact - 1) + i] = freq[i];
        }

    
    }
}