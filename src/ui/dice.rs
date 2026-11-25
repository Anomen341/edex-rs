//! A small xorshift generator. It only shuffles boot jokes and places decorative
//! satellites, so no crate is needed.

pub(crate) struct Dice(u64);

impl Dice {
    pub(crate) fn seeded() -> Self {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
        Self::new(now.map_or(1, |time| time.as_nanos() as u64))
    }

    pub(crate) fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub(crate) fn below(&mut self, limit: u64) -> u64 {
        self.next() % limit
    }

    pub(crate) fn between(&mut self, low: f64, high: f64) -> f64 {
        low + (self.next() >> 11) as f64 / (1u64 << 53) as f64 * (high - low)
    }

    /// `count` different items from `pool`.
    pub(crate) fn draw<T: Copy>(&mut self, pool: &[T], count: usize) -> Vec<T> {
        let mut items = pool.to_vec();
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i as u64 + 1) as usize);
        }
        items.truncate(count);
        items
    }
}
