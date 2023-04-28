pub struct MLS {
    poly: isize,
    test: isize,
    reg: isize
}

fn hibit (n :isize) -> isize {
    n |= n >> 1;
	n |= n >> 2;
	n |= n >> 4;
	n |= n >> 8;
	n |= n >> 16;
    return n ^ (n >> 1);
}

impl MLS {
    pub fn new(&mut self, poly: isize, reg: isize) -> MLS {
        MLS{
            poly: poly,
            test: hibit(poly) >> 1,
            reg: reg,
        }
    }
    pub fn reset(&mut self) {
        self.reg =1;
    }

    pub fn bad(&mut self) {
        
    }
}