pub const N_PT_PER_FRAME: usize = 4096;

#[repr(C)]
pub struct Payload {
    pub pkt_cnt: u64,
    pub data: [i8; N_PT_PER_FRAME],
}

impl Default for Payload {
    fn default() -> Self {
        Self {
            pkt_cnt: 0,
            data: [0; N_PT_PER_FRAME],
        }
    }
}

impl Payload {
    pub fn copy_header(&mut self, rhs: &Self) {
        self.pkt_cnt = rhs.pkt_cnt;
    }
}
