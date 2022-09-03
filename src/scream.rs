pub const SCREAM_PACKET_MAX_SIZE: usize = 1157;

pub type ScreamPacket = [u8; SCREAM_PACKET_MAX_SIZE];

pub type ScreamHeaderArray = [u8; 5];

pub trait ScreamHeader {
    fn sample_rate(&self) -> u32;
    fn sample_bits(&self) -> u8;
    fn channels(&self) -> u16;
    fn sample_bytes(&self) -> usize {
        return self.sample_bits() as usize / 8;
    }
}

impl ScreamHeader for ScreamHeaderArray {
    fn sample_rate(&self) -> u32 {
        let rate_byte = self[0];
        let multiplier = (rate_byte & 0b01111111) as u32;
        return match rate_byte & 0b10000000 == 0 {
            true => 48000 * multiplier,
            false => 44100 * multiplier,
        };
    }
    fn sample_bits(&self) -> u8 {
        return self[1];
    }
    fn channels(&self) -> u16 {
        return 2; // TODO
    }
}
