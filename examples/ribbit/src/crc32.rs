pub struct CRC32 {
    lut: [u32; 256],
    poly: u32,
    crc: u32
}

impl CRC32 {
    fn update (&mut self, prev: u32, data: u8) -> u32 {
        let mut dat: u32 = 0;
        if data != 0 {
            dat = 1;
        }
        let tmp= prev ^ dat;
        return (prev >> 1) ^ ((tmp & 1) * self.poly);
    }
    
    pub fn new(poly: u32, crc: u32) -> CRC32 {
        let mut ret: CRC32 = CRC32 { lut: [0; 256], 
            poly: poly, 
            crc: crc
        };
    
        for j in 0..256 {
            let mut  tmp = j;
            for i in 0..8 {
                tmp = ret.update(tmp, 0);
            }
            ret.lut[j as usize] = tmp;
        }
        return ret;
    }

    pub fn reset(&mut self) {
        self.crc = 0;
    }

    pub fn crc(&mut self) -> u32{
        return self.crc;
    } 
    pub fn crc_u8(&mut self, data: u8) -> u32 {
        let mut tmp = self.crc ^ (data as u32);
        self.crc = (self.crc >> 8) ^ self.lut[(tmp & 255) as usize];
        return self.crc
    }

    pub fn crc_u16(&mut self, data: u16) -> u32 {
        self.crc_u8(data as u8 & 255);
        self.crc_u8((data >> 8) as u8 & 255);
        return self.crc
    }

    pub fn crc_u32(&mut self, data: u32) -> u32 {
        self.crc_u8(data as u8 & 255);
        self.crc_u8((data >> 8) as u8 & 255);
        self.crc_u8((data >> 16) as u8 & 255);
        self.crc_u8((data >> 24) as u8 & 255);
        return self.crc
    }

    pub fn crc_u64(&mut self, data: u64) -> u32 {
        self.crc_u8(data as u8 & 255);
        self.crc_u8((data >> 8) as u8 & 255);
        self.crc_u8((data >> 16) as u8 & 255);
        self.crc_u8((data >> 24) as u8 & 255);
        self.crc_u8((data >> 32) as u8 & 255);
        self.crc_u8((data >> 40) as u8 & 255);
        self.crc_u8((data >> 48) as u8 & 255);
        self.crc_u8((data >> 56) as u8 & 255);
        return self.crc
    }
}


