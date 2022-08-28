trait ScreamHeader {
    fn sample_rate(&self) -> u32;
    fn sample_size(&self) -> usize;
    fn channels(&self) -> u16;
    fn sample_size_bytes(&self) -> usize;
}

impl ScreamHeader for [u8; 5] {
    fn sample_rate(&self) -> u32 {
        let multiplier = (rate & 0b01111111) as u32;

        return match rate & 0b10000000 == 0 {
            true => 48000 * multiplier,
            false => 44100 * multiplier,
        };
    }

    fn sample_size(&self) -> usize {
        return self[1];
    }

    fn channels(&self) -> u16 {
        return 2; // TODO
    }
}
