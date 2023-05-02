pub struct MLS {
    poly: isize,
    test: isize,
    reg: isize
}

fn hibit (n :isize) -> isize {
    let mut n = n;
    n |= n >> 1;
	n |= n >> 2;
	n |= n >> 4;
	n |= n >> 8;
	n |= n >> 16;
    return n ^ (n >> 1);
}

impl MLS {
    pub fn new(poly: isize, reg: isize) -> MLS {
        MLS{
            poly: poly,
            test: hibit(poly) >> 1,
            reg: reg,
        }
    }

    pub fn mls(&mut self) -> bool {
		let mut fb = self.reg & self.test;
        if fb == 0 {
            fb = 0;
        } else {
            fb = 1;
        }
		
		self.reg <<= 1;
		self.reg ^= fb * self.poly;
        if fb == 0 {
            return false;
        } else {
            return true;
        }
		
    }

    pub fn reset(&mut self) {
        self.reg =1;
    }

    pub fn bad(&mut self) {
        
    }
}