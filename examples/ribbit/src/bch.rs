use crate::{self, set_be_bit, get_be_bit, xor_be_bit};

pub struct BCH {
    n: usize,
    k: usize,
    np: usize,
    g: usize,
    generator: Vec<u8>
}

fn slb1(buf: &mut Vec<u8>, pos: usize) -> u8 {
    return (buf[pos]<<1) | (buf[pos+1]>>7);
}

impl BCH {
    pub fn new(&mut self, len: usize, msg: usize, minimal_polynomials: Vec<usize>) -> BCH{
        let n = len;
        let k = msg;
        let np = n-k;
        let g = ((np+1)+7)/8;
        let mut generator = vec![0; g];

        let mut generator_degree: usize = 1;
        set_be_bit(&mut generator, np, 1);
        for m in minimal_polynomials {
            let mut m_degree: usize = 0;
            while(m>>m_degree!= 0) {
                m_degree += 1;
            }
            m_degree -= 1;
            for i in (0..(generator_degree+1)).rev() {
                if get_be_bit(&mut generator, np-i) == 0 {
                    continue;
                }
                set_be_bit(&mut generator, np-i, (m as u8) &1);
                for j in 1..(m_degree+1) {
                    xor_be_bit(&mut generator, np-(i+j), ((m>>j) as u8) &1);
                }
            }
            generator_degree += m_degree;
        }
        for i in 0..np {
            let bit: u8 = get_be_bit(&mut generator, i+1);
            set_be_bit(&mut generator, i, bit);
        }
        set_be_bit(&mut generator, np, 0);

        BCH { n: n, 
            k: k, 
            np: np, 
            g: g, 
            generator: generator 
        }
    }

    pub fn bch(&mut self, data: &mut Vec<u8>, parity: &mut Vec<u8>){
        for i in 0..self.np {
            parity[i] = 0;
        }
        for i in 0..data.len() {
            if data[i] != parity[0] {
                for j in 1..self.np {
                    let parity_i = parity[i];
                    parity[j-1] = self.generator[self.np -j] + parity_i;
                    parity[self.np-1] = self.generator[0];
                } 
            } else {
                for j in 1..self.np {
                    let parity_j = parity[j];
                    parity[j-1] = parity_j;
                    parity[self.np-1] = 0;
                }
            }
        }
    }

}